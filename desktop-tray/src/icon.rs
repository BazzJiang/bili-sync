//! 程序内生成托盘图标，避免引入图片解码依赖和额外资源文件。
//!
//! 设计：圆角方块 + 中间镂空的播放三角（媒体库的通用符号）。
//! macOS 用 template 图标（纯黑 + alpha），系统会按明暗菜单栏自动反色；
//! 其它平台用 B 站粉底 + 白色三角，在深色任务栏上也能看清。

use tray_icon::Icon;

/// 托盘图标边长。32px 在 Windows/Linux 托盘和 macOS 菜单栏下都清晰。
const SIZE: u32 = 32;
/// 每像素采样数（每个轴），用于抗锯齿。
const SUPERSAMPLE: u32 = 4;

const BG_HALF: f32 = 0.44;
const BG_RADIUS: f32 = 0.16;

// 播放三角的三个顶点（归一化坐标，y 轴向下）
const TRIANGLE: [(f32, f32); 3] = [(0.40, 0.28), (0.40, 0.72), (0.72, 0.50)];

/// 生成图标，并返回是否需要按 template 处理（macOS）。
pub fn build() -> Result<(Icon, bool), String> {
    let template = cfg!(target_os = "macos");
    // template 图标只由 alpha 决定形状；非 template 用品牌色填充
    let bg: [u8; 3] = if template { [0, 0, 0] } else { [0xfb, 0x72, 0x99] };
    let fg: [u8; 3] = if template { [0, 0, 0] } else { [255, 255, 255] };

    let mut rgba = vec![0u8; (SIZE * SIZE * 4) as usize];
    for y in 0..SIZE {
        for x in 0..SIZE {
            let (bg_cov, glyph_cov) = coverage(x, y);
            if bg_cov <= 0.0 {
                continue;
            }
            let idx = ((y * SIZE + x) * 4) as usize;
            // template：镂空部分直接扣 alpha
            // 非 template：按覆盖率把前景色混进底色
            let (color, alpha) = if template {
                (bg, bg_cov - glyph_cov)
            } else {
                let glyph_ratio = glyph_cov / bg_cov;
                let mix = |a: u8, b: u8| (a as f32 * (1.0 - glyph_ratio) + b as f32 * glyph_ratio) as u8;
                ([mix(bg[0], fg[0]), mix(bg[1], fg[1]), mix(bg[2], fg[2])], bg_cov)
            };
            rgba[idx] = color[0];
            rgba[idx + 1] = color[1];
            rgba[idx + 2] = color[2];
            rgba[idx + 3] = (alpha.clamp(0.0, 1.0) * 255.0).round() as u8;
        }
    }

    let icon = Icon::from_rgba(rgba, SIZE, SIZE).map_err(|e| format!("生成托盘图标失败：{e}"))?;
    Ok((icon, template))
}

/// 计算某像素上「底色」和「三角」的覆盖比例（0.0 - 1.0）。
fn coverage(x: u32, y: u32) -> (f32, f32) {
    let mut bg_hits = 0u32;
    let mut glyph_hits = 0u32;
    for sy in 0..SUPERSAMPLE {
        for sx in 0..SUPERSAMPLE {
            let px = (x as f32 + (sx as f32 + 0.5) / SUPERSAMPLE as f32) / SIZE as f32;
            let py = (y as f32 + (sy as f32 + 0.5) / SUPERSAMPLE as f32) / SIZE as f32;
            if inside_rounded_rect(px, py) {
                bg_hits += 1;
                if inside_triangle(px, py) {
                    glyph_hits += 1;
                }
            }
        }
    }
    let total = (SUPERSAMPLE * SUPERSAMPLE) as f32;
    (bg_hits as f32 / total, glyph_hits as f32 / total)
}

fn inside_rounded_rect(x: f32, y: f32) -> bool {
    // 圆角矩形的近似有向距离：中心距离减去内缩后的半宽，再做圆角修正
    let qx = (x - 0.5).abs() - (BG_HALF - BG_RADIUS);
    let qy = (y - 0.5).abs() - (BG_HALF - BG_RADIUS);
    let outside = qx.max(0.0).hypot(qy.max(0.0)) - BG_RADIUS;
    outside <= 0.0
}

fn inside_triangle(x: f32, y: f32) -> bool {
    // 依次检查点是否在三条边的同一侧
    let sign = |a: (f32, f32), b: (f32, f32)| (x - b.0) * (a.1 - b.1) - (a.0 - b.0) * (y - b.1);
    let d1 = sign(TRIANGLE[0], TRIANGLE[1]);
    let d2 = sign(TRIANGLE[1], TRIANGLE[2]);
    let d3 = sign(TRIANGLE[2], TRIANGLE[0]);
    let has_neg = d1 < 0.0 || d2 < 0.0 || d3 < 0.0;
    let has_pos = d1 > 0.0 || d2 > 0.0 || d3 > 0.0;
    !(has_neg && has_pos)
}

#[cfg(test)]
mod tests {
    use super::build;

    #[test]
    fn builds_icon_with_opaque_pixels_and_transparent_corners() {
        let (icon, _template) = build().expect("icon should build");
        // 只看尺寸：Icon 没有公开的取像素接口，用「不 panic + 角落透明」做基本校验
        let _ = icon;
        let (bg_cov, _glyph_cov) = super::coverage(0, 0);
        assert_eq!(bg_cov, 0.0, "左上角应该完全透明");
        let (bg_cov, glyph_cov) = super::coverage(16, 16);
        assert!(bg_cov > 0.9, "中心应该在底色内");
        assert!(glyph_cov > 0.0, "中心应该覆盖播放三角");
    }
}
