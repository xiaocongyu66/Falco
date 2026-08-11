//! Media — <audio> and <video> element rendering.
//!
//! Falco doesn't play actual audio/video (that would require a media
//! pipeline), but it renders the player UI:
//! - <audio>: a small player bar with play button and progress bar
//! - <video>: a video frame with poster image (if available) + play button overlay
//!
//! In interactive mode, clicking the play button would call JS
//! media APIs (future feature).

use crate::css::Color;
use crate::layout::Rect;
use crate::paint::PaintCommand;

/// Render an <audio> element as a player bar.
/// Returns paint commands for the player UI.
pub fn render_audio_player(
    bounds: &Rect,
    attrs: &std::collections::HashMap<String, String>,
) -> Vec<PaintCommand> {
    let mut commands = Vec::new();
    let x = bounds.x;
    let y = bounds.y;
    let w = bounds.width;
    let h = bounds.height.max(40.0);

    // Background bar (dark grey).
    commands.push(PaintCommand::RoundedRect {
        x,
        y,
        w,
        h,
        color: Color::rgb(35, 35, 35),
        radius: crate::style::BorderRadius {
            top_left: 6.0,
            top_right: 6.0,
            bottom_left: 6.0,
            bottom_right: 6.0,
        },
    });

    // Play button (triangle in a circle).
    let btn_size = h.min(28.0);
    let btn_x = x + 8.0;
    let btn_y = y + (h - btn_size) / 2.0;
    // Circle background.
    commands.push(PaintCommand::RoundedRect {
        x: btn_x,
        y: btn_y,
        w: btn_size,
        h: btn_size,
        color: Color::rgb(80, 80, 80),
        radius: crate::style::BorderRadius {
            top_left: btn_size / 2.0,
            top_right: btn_size / 2.0,
            bottom_left: btn_size / 2.0,
            bottom_right: btn_size / 2.0,
        },
    });
    // Play triangle (simplified — just a small filled triangle).
    let tri_x = btn_x + btn_size * 0.35;
    let tri_y = btn_y + btn_size * 0.25;
    let tri_w = btn_size * 0.3;
    let tri_h = btn_size * 0.5;
    // Draw triangle as three lines + fill (simplified to a small rect for now).
    commands.push(PaintCommand::RoundedRect {
        x: tri_x,
        y: tri_y,
        w: tri_w * 0.7,
        h: tri_h,
        color: Color::rgb(255, 255, 255),
        radius: Default::default(),
    });

    // Progress bar.
    let bar_x = btn_x + btn_size + 12.0;
    let bar_w = w - btn_size - 28.0;
    let bar_y = y + h / 2.0 - 3.0;
    let bar_h = 6.0;
    // Track (lighter grey).
    commands.push(PaintCommand::RoundedRect {
        x: bar_x,
        y: bar_y,
        w: bar_w,
        h: bar_h,
        color: Color::rgb(60, 60, 60),
        radius: crate::style::BorderRadius {
            top_left: 3.0,
            top_right: 3.0,
            bottom_left: 3.0,
            bottom_right: 3.0,
        },
    });
    // Filled portion (10% — no actual playback).
    commands.push(PaintCommand::RoundedRect {
        x: bar_x,
        y: bar_y,
        w: bar_w * 0.1,
        h: bar_h,
        color: Color::rgb(100, 150, 250),
        radius: crate::style::BorderRadius {
            top_left: 3.0,
            top_right: 3.0,
            bottom_left: 3.0,
            bottom_right: 3.0,
        },
    });

    // Time label (0:00 / 0:00).
    let src = attrs.get("src").map(|s| s.as_str()).unwrap_or("audio");
    let label = format!("♪ {}", shorten_filename(src));
    commands.push(PaintCommand::Text {
        x: bar_x,
        y: bar_y - 16.0,
        text: label,
        color: Color::rgb(180, 180, 180),
        font_size: 11.0,
        font_weight: 400,
        italic: false,
        family: "sans-serif".to_string(),
    });

    commands
}

/// Render a <video> element with optional poster image + play button overlay.
pub fn render_video_player(
    bounds: &Rect,
    attrs: &std::collections::HashMap<String, String>,
    base_url: &str,
) -> Vec<PaintCommand> {
    let mut commands = Vec::new();
    let x = bounds.x;
    let y = bounds.y;
    let w = bounds.width;
    let h = bounds.height.max(100.0);

    // Background (black).
    commands.push(PaintCommand::RoundedRect {
        x,
        y,
        w,
        h,
        color: Color::rgb(0, 0, 0),
        radius: Default::default(),
    });

    // Try to load poster image.
    if let Some(poster) = attrs.get("poster") {
        if let Some(img) = crate::image::load_image(poster, base_url) {
            commands.push(PaintCommand::Image {
                x,
                y,
                w,
                h,
                image: img,
            });
        }
    } else {
        // No poster — draw a placeholder gradient.
        commands.push(PaintCommand::Gradient {
            x,
            y,
            w,
            h,
            angle: 180.0,
            stops: vec![(0.0, Color::rgb(30, 30, 40)), (1.0, Color::rgb(15, 15, 20))],
            opacity: 1.0,
        });
    }

    // Play button overlay (large centered circle with triangle).
    let btn_r = w.min(h).min(80.0) * 0.15;
    let cx = x + w / 2.0;
    let cy = y + h / 2.0;
    // Semi-transparent circle.
    commands.push(PaintCommand::RoundedRect {
        x: cx - btn_r,
        y: cy - btn_r,
        w: btn_r * 2.0,
        h: btn_r * 2.0,
        color: Color::rgba(0, 0, 0, 120),
        radius: crate::style::BorderRadius {
            top_left: btn_r,
            top_right: btn_r,
            bottom_left: btn_r,
            bottom_right: btn_r,
        },
    });
    // Play triangle (simplified as a small white rect).
    let tri_w = btn_r * 0.5;
    let tri_h = btn_r * 0.7;
    commands.push(PaintCommand::RoundedRect {
        x: cx - tri_w * 0.2,
        y: cy - tri_h / 2.0,
        w: tri_w,
        h: tri_h,
        color: Color::rgb(255, 255, 255),
        radius: Default::default(),
    });

    // Source label at bottom.
    let src = attrs.get("src").map(|s| s.as_str()).unwrap_or("video");
    let label = format!("▶ {}", shorten_filename(src));
    commands.push(PaintCommand::Text {
        x: x + 8.0,
        y: y + h - 24.0,
        text: label,
        color: Color::rgb(200, 200, 200),
        font_size: 12.0,
        font_weight: 400,
        italic: false,
        family: "sans-serif".to_string(),
    });

    commands
}

fn shorten_filename(s: &str) -> String {
    let s = s.rsplit('/').next().unwrap_or(s);
    if s.len() > 40 {
        format!("{}...", &s[..37])
    } else {
        s.to_string()
    }
}
