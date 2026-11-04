//! Post-processing pipeline. The whole game (world + UI) is drawn into an offscreen target at
//! the window's real resolution (optionally 2x supersampled), then presented through a CRT
//! shader: bloom/glow, barrel warp, scanlines, vignette and a touch of chromatic aberration.
use crate::graphics::{Graphics, View};
use crate::objects::{SCREEN_H, SCREEN_W};
use macroquad::miniquad::{ShaderSource, UniformDesc, UniformType};
use macroquad::prelude::*;

const VERTEX: &str = r#"#version 100
attribute vec3 position;
attribute vec2 texcoord;
attribute vec4 color0;
varying highp vec2 uv;
uniform mat4 Model;
uniform mat4 Projection;
void main() {
    gl_Position = Projection * Model * vec4(position, 1);
    uv = texcoord;
}"#;

const BLUR_FRAGMENT: &str = r#"#version 100
precision highp float;
varying highp vec2 uv;
uniform sampler2D Texture;
uniform vec2 Dir;
void main() {
    vec4 c = texture2D(Texture, uv) * 0.227027;
    c += texture2D(Texture, uv + Dir * 1.0) * 0.1945946;
    c += texture2D(Texture, uv - Dir * 1.0) * 0.1945946;
    c += texture2D(Texture, uv + Dir * 2.0) * 0.1216216;
    c += texture2D(Texture, uv - Dir * 2.0) * 0.1216216;
    c += texture2D(Texture, uv + Dir * 3.0) * 0.054054;
    c += texture2D(Texture, uv - Dir * 3.0) * 0.054054;
    c += texture2D(Texture, uv + Dir * 4.0) * 0.016216;
    c += texture2D(Texture, uv - Dir * 4.0) * 0.016216;
    gl_FragColor = vec4(c.rgb, 1.0);
}"#;

const COMPOSITE_FRAGMENT: &str = r#"#version 100
precision highp float;
varying highp vec2 uv;
uniform sampler2D Texture;
uniform sampler2D Bloom1;
uniform sampler2D Bloom2;
uniform float Curve;
uniform float Scan;
uniform float BloomAmt;
uniform float OutH;
void main() {
    // Barrel warp: output position p in [-1,1] -> source position w, edges stay put.
    vec2 p = uv * 2.0 - 1.0;
    vec2 w = p * (1.0 + Curve * dot(p, p)) / (1.0 + Curve);
    vec2 suv = w * 0.5 + 0.5;
    float edge = min(min(suv.x, 1.0 - suv.x), min(suv.y, 1.0 - suv.y));
    if (edge < 0.0) { gl_FragColor = vec4(0.0, 0.0, 0.0, 1.0); return; }

    // Slight chromatic aberration that grows towards the edges.
    vec2 ab = w * (0.0006 + 0.0014 * Curve);
    vec3 col;
    col.r = texture2D(Texture, suv + ab).r;
    col.g = texture2D(Texture, suv).g;
    col.b = texture2D(Texture, suv - ab).b;

    // Glow.
    col += (texture2D(Bloom1, suv).rgb * 0.8 + texture2D(Bloom2, suv).rgb * 1.2) * BloomAmt;

    // Scanlines locked to physical pixel rows.
    float period = max(2.0, floor(OutH / 240.0 + 0.5));
    float s = 0.5 + 0.5 * cos(gl_FragCoord.y / period * 6.2831853);
    col *= 1.0 - Scan * (1.0 - s);

    // Vignette and a soft, anti-aliased screen border.
    col *= 1.0 - dot(w, w) * (0.10 + 0.5 * Curve);
    col *= smoothstep(0.0, 0.004, edge);
    gl_FragColor = vec4(col, 1.0);
}"#;

struct Targets {
    scene: RenderTarget,
    half_a: RenderTarget,
    half_b: RenderTarget,
    quarter_a: RenderTarget,
    quarter_b: RenderTarget,
    w: u32,
    h: u32,
}

fn target(w: u32, h: u32) -> RenderTarget {
    let rt = render_target(w.max(1), h.max(1));
    rt.texture.set_filter(FilterMode::Linear);
    rt
}

impl Targets {
    fn new(w: u32, h: u32) -> Self {
        Self {
            scene: target(w, h),
            half_a: target(w / 2, h / 2),
            half_b: target(w / 2, h / 2),
            quarter_a: target(w / 4, h / 4),
            quarter_b: target(w / 4, h / 4),
            w,
            h,
        }
    }
}

/// Camera that draws into `rt` using the game's top-left-origin coordinates `0..w, 0..h`.
fn camera_for(rt: &RenderTarget, w: f32, h: f32) -> Camera2D {
    Camera2D { render_target: Some(rt.clone()), ..Camera2D::from_display_rect(Rect::new(0.0, 0.0, w, h)) }
}

pub struct PostFx {
    blur: Material,
    composite: Material,
    targets: Option<Targets>,
}

impl PostFx {
    pub fn new() -> Self {
        let blur = load_material(
            ShaderSource::Glsl { vertex: VERTEX, fragment: BLUR_FRAGMENT },
            MaterialParams { uniforms: vec![UniformDesc::new("Dir", UniformType::Float2)], ..Default::default() },
        )
        .expect("blur shader");
        let composite = load_material(
            ShaderSource::Glsl { vertex: VERTEX, fragment: COMPOSITE_FRAGMENT },
            MaterialParams {
                uniforms: ["Curve", "Scan", "BloomAmt", "OutH"].iter().map(|n| UniformDesc::new(n, UniformType::Float1)).collect(),
                textures: vec!["Bloom1".to_string(), "Bloom2".to_string()],
                ..Default::default()
            },
        )
        .expect("composite shader");
        Self { blur, composite, targets: None }
    }

    /// Pixels per game unit used for the offscreen scene (text is rasterised at this scale).
    fn render_scale(view: &View, supersample: bool) -> f32 {
        let rs = if supersample { view.scale * 2.0 } else { view.scale };
        ((rs.clamp(1.0, 4.0) * 4.0).ceil()) / 4.0 // quantised so resizing doesn't thrash the targets
    }

    /// Start drawing a frame. Everything drawn until `present` goes to the offscreen scene.
    /// Returns the render scale so the UI can rasterise text sharply.
    pub fn begin(&mut self, view: &View, gfx: &Graphics) -> f32 {
        let rs = Self::render_scale(view, gfx.antialiasing);
        let (w, h) = ((SCREEN_W * rs) as u32, (SCREEN_H * rs) as u32);
        if self.targets.as_ref().map(|t| (t.w, t.h)) != Some((w, h)) {
            self.targets = Some(Targets::new(w, h));
        }
        let t = self.targets.as_ref().unwrap();
        set_camera(&camera_for(&t.scene, SCREEN_W, SCREEN_H));
        clear_background(BLACK);
        rs
    }

    fn pass(&self, src: &RenderTarget, dst: &RenderTarget, dir: Vec2) {
        let (w, h) = (dst.texture.width(), dst.texture.height());
        set_camera(&camera_for(dst, w, h));
        self.blur.set_uniform("Dir", dir);
        gl_use_material(&self.blur);
        draw_texture_ex(&src.texture, 0.0, 0.0, WHITE, DrawTextureParams { dest_size: Some(vec2(w, h)), flip_y: true, ..Default::default() });
        gl_use_default_material();
    }

    /// Finish the frame: blur for glow, then draw the CRT composite into the letterboxed area.
    pub fn present(&mut self, view: &View, gfx: &Graphics) {
        let Some(t) = self.targets.as_ref() else { return };
        let bloom = gfx.bloom_amount();

        if bloom > 0.0 {
            let sw = t.scene.texture.width();
            let (hw, hh) = (t.half_a.texture.width(), t.half_a.texture.height());
            let qh = t.quarter_a.texture.height();
            self.pass(&t.scene, &t.half_a, vec2(2.0 / sw, 0.0));
            self.pass(&t.half_a, &t.half_b, vec2(0.0, 2.0 / hh));
            self.pass(&t.half_b, &t.quarter_a, vec2(2.0 / hw, 0.0));
            self.pass(&t.quarter_a, &t.quarter_b, vec2(0.0, 2.0 / qh));
        }

        set_default_camera();
        let m = &self.composite;
        m.set_uniform("Curve", gfx.curvature_amount());
        m.set_uniform("Scan", gfx.scanline_amount());
        m.set_uniform("BloomAmt", bloom);
        m.set_uniform("OutH", SCREEN_H * view.scale);
        m.set_texture("Bloom1", t.half_b.texture.clone());
        m.set_texture("Bloom2", t.quarter_b.texture.clone());
        gl_use_material(m);
        draw_texture_ex(
            &t.scene.texture,
            view.offset.x,
            view.offset.y,
            WHITE,
            DrawTextureParams { dest_size: Some(vec2(SCREEN_W * view.scale, SCREEN_H * view.scale)), flip_y: true, ..Default::default() },
        );
        gl_use_default_material();
    }
}
