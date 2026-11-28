//! Bounded image validation and Windows WIC container decoding.

use sha2::{Digest, Sha256};

/// Maximum encoded provider response per image.
pub const MAX_OUTPUT_BYTES: usize = 8 * 1024 * 1024;
/// Maximum output width or height.
pub const MAX_DIMENSION: u32 = 4096;
/// Maximum decoded output pixel count.
pub const MAX_PIXELS: u64 = 16 * 1024 * 1024;
/// Maximum decoded RGBA bytes allocated during validation.
pub const MAX_DECODED_BYTES: usize = 64 * 1024 * 1024;

/// MIME type accepted after byte-signature and platform decoder validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageMimeType {
    /// PNG encoded image.
    Png,
    /// JPEG encoded image.
    Jpeg,
}

impl ImageMimeType {
    /// Returns the canonical MIME type for IPC and artifact metadata.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Png => "image/png",
            Self::Jpeg => "image/jpeg",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value {
            "image/png" => Some(Self::Png),
            "image/jpeg" => Some(Self::Jpeg),
            _ => None,
        }
    }
}

/// Validated metadata and original encoded bytes for one provider image.
#[derive(Debug)]
pub struct ValidatedImage {
    /// Canonical MIME type verified against the decoded container.
    pub mime_type: ImageMimeType,
    /// Decoded width in pixels.
    pub width: u32,
    /// Decoded height in pixels.
    pub height: u32,
    /// SHA-256 digest over the original encoded bytes.
    pub sha256: String,
    /// Original encoded image bytes.
    pub bytes: Vec<u8>,
}

/// Typed refusal from image validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageValidationError {
    /// Request or output exceeds a byte or dimension bound.
    LimitExceeded,
    /// Declared MIME does not match an allow-listed image container.
    MimeMismatch,
    /// Input bytes do not contain a supported PNG or JPEG signature.
    InvalidSignature,
    /// Native decoder could not validate the complete image.
    DecodeFailed,
    /// Current operating system has no supported image decoder.
    DecoderUnavailable,
    /// Decoded dimensions or output allocation exceed the pixel bound.
    DimensionsExceeded,
}

impl std::fmt::Display for ImageValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for ImageValidationError {}

/// Validates an encoded image by declared MIME, byte signature, decoded dimensions, and pixels.
pub fn validate_output(
    declared_mime: &str,
    bytes: Vec<u8>,
) -> Result<ValidatedImage, ImageValidationError> {
    if bytes.is_empty() || bytes.len() > MAX_OUTPUT_BYTES {
        return Err(ImageValidationError::LimitExceeded);
    }
    let mime = ImageMimeType::parse(declared_mime).ok_or(ImageValidationError::MimeMismatch)?;
    if signature_mime(&bytes) != Some(mime) {
        return Err(ImageValidationError::InvalidSignature);
    }
    let (width, height) = decode_dimensions(&bytes, mime)?;
    if width == 0 || height == 0 || width > MAX_DIMENSION || height > MAX_DIMENSION {
        return Err(ImageValidationError::DimensionsExceeded);
    }
    let pixels = u64::from(width)
        .checked_mul(u64::from(height))
        .ok_or(ImageValidationError::DimensionsExceeded)?;
    let decoded_bytes = pixels
        .checked_mul(4)
        .ok_or(ImageValidationError::DimensionsExceeded)?;
    if pixels > MAX_PIXELS || decoded_bytes > MAX_DECODED_BYTES as u64 {
        return Err(ImageValidationError::DimensionsExceeded);
    }
    Ok(ValidatedImage {
        mime_type: mime,
        width,
        height,
        sha256: hex::encode(Sha256::digest(&bytes)),
        bytes,
    })
}

fn signature_mime(bytes: &[u8]) -> Option<ImageMimeType> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some(ImageMimeType::Png)
    } else if bytes.starts_with(b"\xff\xd8\xff") {
        Some(ImageMimeType::Jpeg)
    } else {
        None
    }
}

#[cfg(windows)]
fn decode_dimensions(
    bytes: &[u8],
    mime: ImageMimeType,
) -> Result<(u32, u32), ImageValidationError> {
    use windows::Win32::Graphics::Imaging::{
        CLSID_WICImagingFactory, GUID_ContainerFormatJpeg, GUID_ContainerFormatPng,
        GUID_WICPixelFormat32bppRGBA, IWICImagingFactory, WICBitmapDitherTypeNone,
        WICBitmapPaletteTypeCustom, WICDecodeMetadataCacheOnDemand,
    };
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER,
        COINIT_MULTITHREADED,
    };

    let initialized = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }
        .ok()
        .is_ok();
    if !initialized {
        return Err(ImageValidationError::DecodeFailed);
    }
    let result = (|| unsafe {
        let factory: IWICImagingFactory =
            CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)
                .map_err(|_| ImageValidationError::DecodeFailed)?;
        let stream = factory
            .CreateStream()
            .map_err(|_| ImageValidationError::DecodeFailed)?;
        stream
            .InitializeFromMemory(bytes)
            .map_err(|_| ImageValidationError::DecodeFailed)?;
        let decoder = factory
            .CreateDecoderFromStream(&stream, std::ptr::null(), WICDecodeMetadataCacheOnDemand)
            .map_err(|_| ImageValidationError::DecodeFailed)?;
        let expected_container = match mime {
            ImageMimeType::Png => GUID_ContainerFormatPng,
            ImageMimeType::Jpeg => GUID_ContainerFormatJpeg,
        };
        if decoder
            .GetContainerFormat()
            .map_err(|_| ImageValidationError::DecodeFailed)?
            != expected_container
        {
            return Err(ImageValidationError::MimeMismatch);
        }
        if decoder
            .GetFrameCount()
            .map_err(|_| ImageValidationError::DecodeFailed)?
            != 1
        {
            return Err(ImageValidationError::DecodeFailed);
        }
        let frame = decoder
            .GetFrame(0)
            .map_err(|_| ImageValidationError::DecodeFailed)?;
        let mut width = 0;
        let mut height = 0;
        frame
            .GetSize(&mut width, &mut height)
            .map_err(|_| ImageValidationError::DecodeFailed)?;
        let pixels = u64::from(width)
            .checked_mul(u64::from(height))
            .ok_or(ImageValidationError::DimensionsExceeded)?;
        let rgba_bytes = pixels
            .checked_mul(4)
            .ok_or(ImageValidationError::DimensionsExceeded)?;
        if width == 0
            || height == 0
            || width > MAX_DIMENSION
            || height > MAX_DIMENSION
            || pixels > MAX_PIXELS
            || rgba_bytes > MAX_DECODED_BYTES as u64
        {
            return Err(ImageValidationError::DimensionsExceeded);
        }
        let converter = factory
            .CreateFormatConverter()
            .map_err(|_| ImageValidationError::DecodeFailed)?;
        converter
            .Initialize(
                &frame,
                &GUID_WICPixelFormat32bppRGBA,
                WICBitmapDitherTypeNone,
                None,
                0.0,
                WICBitmapPaletteTypeCustom,
            )
            .map_err(|_| ImageValidationError::DecodeFailed)?;
        let stride = width
            .checked_mul(4)
            .ok_or(ImageValidationError::DimensionsExceeded)?;
        let buffer_len =
            usize::try_from(rgba_bytes).map_err(|_| ImageValidationError::DimensionsExceeded)?;
        let mut rgba = vec![0u8; buffer_len];
        converter
            .CopyPixels(std::ptr::null(), stride, &mut rgba)
            .map_err(|_| ImageValidationError::DecodeFailed)?;
        Ok((width, height))
    })();
    unsafe { CoUninitialize() };
    result
}

#[cfg(not(windows))]
fn decode_dimensions(
    _bytes: &[u8],
    _mime: ImageMimeType,
) -> Result<(u32, u32), ImageValidationError> {
    Err(ImageValidationError::DecoderUnavailable)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsupported_or_mismatched_mime_and_raw_bytes_fail_closed() {
        assert!(matches!(
            validate_output("image/jpeg", b"\x89PNG\r\n\x1a\n".to_vec()),
            Err(ImageValidationError::InvalidSignature)
        ));
        assert!(matches!(
            validate_output("image/webp", b"RIFF0000WEBP".to_vec()),
            Err(ImageValidationError::MimeMismatch)
        ));
        assert!(matches!(
            validate_output("image/png", vec![0; MAX_OUTPUT_BYTES + 1]),
            Err(ImageValidationError::LimitExceeded)
        ));
    }

    #[cfg(windows)]
    #[test]
    fn valid_1x1_png_decodes_with_wic() {
        use base64::Engine;
        let png = base64::engine::general_purpose::STANDARD
            .decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAIAAACQd1PeAAAADUlEQVR4nGP4z8AAAAMBAQDJ/pLvAAAAAElFTkSuQmCC")
            .expect("base64 png");
        let validated = validate_output("image/png", png).expect("valid 1x1 png");
        assert_eq!(validated.mime_type, ImageMimeType::Png);
        assert_eq!(validated.width, 1);
        assert_eq!(validated.height, 1);
    }
}
