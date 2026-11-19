use crate::ArtError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

#[derive(Debug)]
pub struct Raster;

impl Raster {
    pub fn render(svg: &[u8], size_px: u32) -> Result<Image, ArtError> {
        if size_px == 0 {
            return Err(ArtError::InvalidSize(size_px));
        }

        let options = resvg::usvg::Options::default();
        let tree = resvg::usvg::Tree::from_data(svg, &options)
            .map_err(|error| ArtError::Svg(error.to_string()))?;
        let mut pixmap = resvg::tiny_skia::Pixmap::new(size_px, size_px)
            .ok_or(ArtError::InvalidSize(size_px))?;

        let scale = size_px as f32 / tree.size().width();
        let transform = resvg::tiny_skia::Transform::from_scale(scale, scale);
        pixmap.fill(resvg::tiny_skia::Color::from_rgba8(0, 0, 0, 0));
        resvg::render(&tree, transform, &mut pixmap.as_mut());

        let pixels = pixmap.data().to_vec();
        Ok(Image {
            width: size_px,
            height: size_px,
            pixels,
        })
    }
}
