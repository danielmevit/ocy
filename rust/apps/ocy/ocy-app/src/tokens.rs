//! Ocy design tokens mapped to overlay colors.

#[rustfmt::skip]
#[allow(dead_code)]
#[path = "../../theme/generated/tokens.rs"]
mod generated;

/// Edge-frame thickness in DIPs for non-active monitors during short breaks and warnings.
pub const EDGE_FRAME_THICKNESS_DIP: u32 = 3;

/// Converts a generated [`damt_theme::Rgba`] token to [`damt_overlay::Rgba`].
#[must_use]
pub const fn to_overlay_rgba(color: damt_theme::Rgba) -> damt_overlay::Rgba {
    damt_overlay::Rgba::new(color.r, color.g, color.b, color.a)
}

/// Green accent edge-frame color (55% alpha) from Ocy's `tokens.rs`.
#[must_use]
pub const fn edge_frame_color(dark: bool) -> damt_overlay::Rgba {
    if dark {
        to_overlay_rgba(generated::EDGE_FRAME_DARK)
    } else {
        to_overlay_rgba(generated::EDGE_FRAME_LIGHT)
    }
}

/// Clay attention edge-frame color (55% alpha) from Ocy's `tokens.rs`.
#[must_use]
pub const fn attention_frame_color(dark: bool) -> damt_overlay::Rgba {
    if dark {
        to_overlay_rgba(generated::ATTENTION_FRAME_DARK)
    } else {
        to_overlay_rgba(generated::ATTENTION_FRAME_LIGHT)
    }
}

/// Long-break full-screen dim overlay color from Ocy's `tokens.rs`.
#[must_use]
pub const fn overlay_dim_color(dark: bool) -> damt_overlay::Rgba {
    if dark {
        to_overlay_rgba(generated::OVERLAY_DARK)
    } else {
        to_overlay_rgba(generated::OVERLAY_LIGHT)
    }
}
