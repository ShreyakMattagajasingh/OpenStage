//! Offscreen screenshot support for PNG export and agent visual checks.

#[derive(Debug, Clone)]
pub struct RgbaScreenshot {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

impl RgbaScreenshot {
    pub fn new(width: u32, height: u32, pixels: Vec<u8>) -> anyhow::Result<Self> {
        let expected = width as usize * height as usize * 4;
        if pixels.len() != expected {
            anyhow::bail!(
                "screenshot pixel length {} did not match expected {} for {}x{}",
                pixels.len(),
                expected,
                width,
                height
            );
        }
        Ok(Self {
            width,
            height,
            pixels,
        })
    }

    pub fn is_non_empty(&self) -> bool {
        self.pixels.chunks_exact(4).any(|px| px[3] != 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_pixel_length() {
        assert!(RgbaScreenshot::new(2, 2, vec![0; 16]).is_ok());
        assert!(RgbaScreenshot::new(2, 2, vec![0; 15]).is_err());
    }

    #[test]
    fn detects_non_empty_alpha() {
        let mut pixels = vec![0; 16];
        assert!(!RgbaScreenshot::new(2, 2, pixels.clone())
            .unwrap()
            .is_non_empty());
        pixels[3] = 255;
        assert!(RgbaScreenshot::new(2, 2, pixels).unwrap().is_non_empty());
    }
}
