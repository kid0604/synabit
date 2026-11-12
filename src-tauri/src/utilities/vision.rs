//! Apple's Vision framework: the subject of a photo, and the text in a scan.
//!
//! Both run on the Neural Engine with models that ship in macOS, so there is
//! nothing to download and nothing leaves the machine (design §3.2: RMBG-2.0 is
//! non-commercial and 490 MB; Vision is already here).
//!
//! - **Subject:** `VNGenerateForegroundInstanceMaskRequest`, macOS 14 or later —
//!   the same "lift subject" Photos does. The mask comes back at the picture's
//!   size, one byte a pixel; the WebView puts it in the picture's alpha.
//! - **Text:** `VNRecognizeTextRequest`, with Vietnamese where this macOS
//!   offers it. Lines come back with where they sit on the page, which is what
//!   `searchable` needs to make a scan searchable.
//!
//! # No new crate
//!
//! There are bindings for Vision (`objc2-vision`), but they are not in the
//! build, and this needs four classes and six messages. So it talks to Vision
//! through `objc2`'s `msg_send!`, which is already here, and to CoreGraphics and
//! CoreVideo through a handful of C functions. Every pointer type that crosses
//! into Objective-C carries its real type encoding, so `objc2`'s checks in a
//! debug build compare like with like.
//!
//! Pictures arrive as RGBA pixels drawn by the WebView, already upright — the
//! WebView applied EXIF orientation when it decoded them — so Vision is always
//! told "up" and the mask lines up with what the person sees.

use std::ffi::{c_void, CStr};
use std::ptr::NonNull;

use objc2::encode::{Encode, Encoding, RefEncode};
use objc2::rc::{autoreleasepool, Allocated, Retained};
use objc2::runtime::{AnyClass, AnyObject, Bool};
use objc2::{class, msg_send};

use super::searchable::{reading_order, Line};

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum VisionError {
    #[error("this needs macOS 14 or later")]
    Unavailable,
    #[error("Vision could not read the picture: {0}")]
    Failed(String),
    /// Nothing that looks like a subject.
    #[error("no subject was found")]
    NoSubject,
}

// ── Opaque C types, with the encodings Objective-C gives them ─────────────

#[repr(C)]
struct CGImage {
    _private: [u8; 0],
}
unsafe impl RefEncode for CGImage {
    const ENCODING_REF: Encoding = Encoding::Pointer(&Encoding::Struct("CGImage", &[]));
}

#[repr(C)]
struct CVBuffer {
    _private: [u8; 0],
}
unsafe impl RefEncode for CVBuffer {
    const ENCODING_REF: Encoding = Encoding::Pointer(&Encoding::Struct("__CVBuffer", &[]));
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct CGPoint {
    x: f64,
    y: f64,
}
unsafe impl Encode for CGPoint {
    const ENCODING: Encoding = Encoding::Struct("CGPoint", &[f64::ENCODING, f64::ENCODING]);
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct CGSize {
    width: f64,
    height: f64,
}
unsafe impl Encode for CGSize {
    const ENCODING: Encoding = Encoding::Struct("CGSize", &[f64::ENCODING, f64::ENCODING]);
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct CGRect {
    origin: CGPoint,
    size: CGSize,
}
unsafe impl Encode for CGRect {
    const ENCODING: Encoding = Encoding::Struct("CGRect", &[CGPoint::ENCODING, CGSize::ENCODING]);
}

#[link(name = "Vision", kind = "framework")]
extern "C" {}

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGColorSpaceCreateDeviceRGB() -> *mut c_void;
    fn CGColorSpaceRelease(space: *mut c_void);
    fn CGDataProviderCreateWithData(
        info: *mut c_void,
        data: *const c_void,
        size: usize,
        release: Option<unsafe extern "C" fn(*mut c_void, *const c_void, usize)>,
    ) -> *mut c_void;
    fn CGDataProviderRelease(provider: *mut c_void);
    #[allow(clippy::too_many_arguments)]
    fn CGImageCreate(
        width: usize,
        height: usize,
        bits_per_component: usize,
        bits_per_pixel: usize,
        bytes_per_row: usize,
        space: *mut c_void,
        bitmap_info: u32,
        provider: *mut c_void,
        decode: *const f64,
        should_interpolate: bool,
        intent: i32,
    ) -> *mut CGImage;
    fn CGImageRelease(image: *mut CGImage);
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFRelease(cf: *const c_void);
}

#[link(name = "CoreVideo", kind = "framework")]
extern "C" {
    fn CVPixelBufferLockBaseAddress(buffer: *mut CVBuffer, flags: u64) -> i32;
    fn CVPixelBufferUnlockBaseAddress(buffer: *mut CVBuffer, flags: u64) -> i32;
    fn CVPixelBufferGetBaseAddress(buffer: *mut CVBuffer) -> *mut c_void;
    fn CVPixelBufferGetBytesPerRow(buffer: *mut CVBuffer) -> usize;
    fn CVPixelBufferGetWidth(buffer: *mut CVBuffer) -> usize;
    fn CVPixelBufferGetHeight(buffer: *mut CVBuffer) -> usize;
    fn CVPixelBufferGetPixelFormatType(buffer: *mut CVBuffer) -> u32;
}

/// `kCGImageAlphaNoneSkipLast`: RGBX, the alpha byte ignored — Vision looks
/// at the picture, not at any transparency it already has.
const SKIP_LAST: u32 = 5;
/// `kCVPixelFormatType_OneComponent32Float` and `…OneComponent8`.
const ONE_FLOAT: u32 = u32::from_be_bytes(*b"L00f");
const ONE_BYTE: u32 = u32::from_be_bytes(*b"L008");
const READ_ONLY: u64 = 1;

/// A CGImage over pixels this function owns for as long as the image lives.
struct Image {
    image: *mut CGImage,
    _pixels: Vec<u8>,
}

impl Image {
    fn new(width: u32, height: u32, rgba: Vec<u8>) -> Result<Self, VisionError> {
        let (w, h) = (width as usize, height as usize);
        // Checked: the size comes from the WebView. Nothing here needs more
        // than about 100 megapixels.
        let bytes = w.checked_mul(h).filter(|&p| p <= 100 << 20).map(|p| p * 4);
        if w == 0 || h == 0 || bytes != Some(rgba.len()) {
            return Err(VisionError::Failed("the picture is the wrong size".into()));
        }
        unsafe {
            let space = CGColorSpaceCreateDeviceRGB();
            let provider = CGDataProviderCreateWithData(std::ptr::null_mut(), rgba.as_ptr().cast(), rgba.len(), None);
            let image = CGImageCreate(w, h, 8, 32, w * 4, space, SKIP_LAST, provider, std::ptr::null(), false, 0);
            CGDataProviderRelease(provider);
            CGColorSpaceRelease(space);
            if image.is_null() {
                return Err(VisionError::Failed("CoreGraphics refused the picture".into()));
            }
            Ok(Self { image, _pixels: rgba })
        }
    }
}

impl Drop for Image {
    fn drop(&mut self) {
        unsafe { CGImageRelease(self.image) }
    }
}

fn class_named(name: &CStr) -> Result<&'static AnyClass, VisionError> {
    AnyClass::get(name).ok_or(VisionError::Unavailable)
}

/// The text of an `NSError`, for a message.
fn describe(error: *mut AnyObject) -> String {
    if error.is_null() {
        return "unknown error".into();
    }
    unsafe {
        let text: Option<Retained<AnyObject>> = msg_send![&*error, localizedDescription];
        text.map(|t| ns_string(&t)).unwrap_or_else(|| "unknown error".into())
    }
}

fn ns_string(s: &AnyObject) -> String {
    unsafe {
        let utf8: *const std::ffi::c_char = msg_send![s, UTF8String];
        if utf8.is_null() { String::new() } else { CStr::from_ptr(utf8).to_string_lossy().into_owned() }
    }
}

fn new_ns_string(s: &str) -> Retained<AnyObject> {
    let c = std::ffi::CString::new(s.replace('\0', "")).unwrap_or_default();
    unsafe {
        let made: Option<Retained<AnyObject>> = msg_send![class!(NSString), stringWithUTF8String: c.as_ptr()];
        made.expect("NSString from UTF-8")
    }
}

/// A request handler for this picture, told it is upright.
fn handler(image: &Image) -> Result<Retained<AnyObject>, VisionError> {
    let cls = class_named(c"VNImageRequestHandler")?;
    unsafe {
        let options: Retained<AnyObject> = msg_send![class!(NSDictionary), dictionary];
        let alloc: Allocated<AnyObject> = msg_send![cls, alloc];
        let made: Option<Retained<AnyObject>> = msg_send![alloc, initWithCGImage: image.image, options: &*options];
        made.ok_or_else(|| VisionError::Failed("no request handler".into()))
    }
}

fn new_request(name: &CStr) -> Result<Retained<AnyObject>, VisionError> {
    let cls = class_named(name)?;
    unsafe {
        let made: Option<Retained<AnyObject>> = msg_send![cls, new];
        made.ok_or_else(|| VisionError::Failed("no request".into()))
    }
}

fn perform(handler: &AnyObject, request: &AnyObject) -> Result<(), VisionError> {
    unsafe {
        let requests: Retained<AnyObject> = msg_send![class!(NSArray), arrayWithObject: request];
        let mut error: *mut AnyObject = std::ptr::null_mut();
        let ok: Bool = msg_send![handler, performRequests: &*requests, error: &mut error];
        if ok.as_bool() { Ok(()) } else { Err(VisionError::Failed(describe(error))) }
    }
}

fn first_result(request: &AnyObject) -> Option<Retained<AnyObject>> {
    unsafe {
        let results: Option<Retained<AnyObject>> = msg_send![request, results];
        let results = results?;
        msg_send![&*results, firstObject]
    }
}

/// What this Mac can do.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Abilities {
    pub subject: bool,
    pub text: bool,
    /// Languages text recognition offers, as Vision names them ("vi-VT").
    pub languages: Vec<String>,
}

pub fn abilities() -> Abilities {
    autoreleasepool(|_| {
        let subject = AnyClass::get(c"VNGenerateForegroundInstanceMaskRequest").is_some();
        let languages = new_request(c"VNRecognizeTextRequest")
            .ok()
            .and_then(|req| unsafe {
                set_accurate(&req);
                let mut error: *mut AnyObject = std::ptr::null_mut();
                let list: Option<Retained<AnyObject>> = msg_send![&*req, supportedRecognitionLanguagesAndReturnError: &mut error];
                list
            })
            .map(|list| ns_strings(&list))
            .unwrap_or_default();
        Abilities { subject, text: !languages.is_empty(), languages }
    })
}

fn ns_strings(array: &AnyObject) -> Vec<String> {
    unsafe {
        let count: usize = msg_send![array, count];
        (0..count)
            .filter_map(|i| {
                let item: Option<Retained<AnyObject>> = msg_send![array, objectAtIndex: i];
                item.map(|s| ns_string(&s))
            })
            .collect()
    }
}

/// The subject of a picture, as one byte a pixel (255 is subject), at the
/// picture's own size.
pub fn subject_mask(width: u32, height: u32, rgba: Vec<u8>) -> Result<Vec<u8>, VisionError> {
    let image = Image::new(width, height, rgba)?;
    autoreleasepool(|_| {
        let request = new_request(c"VNGenerateForegroundInstanceMaskRequest")?;
        let handler = handler(&image)?;
        perform(&handler, &request)?;
        let observation = first_result(&request).ok_or(VisionError::NoSubject)?;
        unsafe {
            let instances: Option<Retained<AnyObject>> = msg_send![&*observation, allInstances];
            let instances = instances.ok_or(VisionError::NoSubject)?;
            let count: usize = msg_send![&*instances, count];
            if count == 0 {
                return Err(VisionError::NoSubject);
            }
            let mut error: *mut AnyObject = std::ptr::null_mut();
            let buffer: *mut CVBuffer = msg_send![
                &*observation,
                generateScaledMaskForImageForInstances: &*instances,
                fromRequestHandler: &*handler,
                error: &mut error
            ];
            let buffer = NonNull::new(buffer).ok_or_else(|| VisionError::Failed(describe(error)))?;
            // The header marks this CF_RETURNS_RETAINED: it is ours to release,
            // whatever reading it says.
            let mask = read_mask(buffer.as_ptr(), width as usize, height as usize);
            CFRelease(buffer.as_ptr() as *const c_void);
            mask
        }
    })
}

/// Copy a one-channel pixel buffer out as bytes, at `width` × `height`.
unsafe fn read_mask(buffer: *mut CVBuffer, width: usize, height: usize) -> Result<Vec<u8>, VisionError> {
    let format = CVPixelBufferGetPixelFormatType(buffer);
    let (bw, bh) = (CVPixelBufferGetWidth(buffer), CVPixelBufferGetHeight(buffer));
    if (bw, bh) != (width, height) {
        return Err(VisionError::Failed(format!("the mask is {bw}×{bh}, the picture {width}×{height}")));
    }
    if CVPixelBufferLockBaseAddress(buffer, READ_ONLY) != 0 {
        return Err(VisionError::Failed("could not read the mask".into()));
    }
    let base = CVPixelBufferGetBaseAddress(buffer) as *const u8;
    let stride = CVPixelBufferGetBytesPerRow(buffer);
    let mut out = Vec::with_capacity(width * height);
    for y in 0..height {
        let row = base.add(y * stride);
        match format {
            ONE_FLOAT => {
                let row = std::slice::from_raw_parts(row as *const f32, width);
                out.extend(row.iter().map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8));
            }
            ONE_BYTE => out.extend_from_slice(std::slice::from_raw_parts(row, width)),
            other => {
                CVPixelBufferUnlockBaseAddress(buffer, READ_ONLY);
                return Err(VisionError::Failed(format!("an unexpected mask format {other:#x}")));
            }
        }
    }
    CVPixelBufferUnlockBaseAddress(buffer, READ_ONLY);
    Ok(out)
}

unsafe fn set_accurate(request: &AnyObject) {
    // VNRequestTextRecognitionLevelAccurate
    let _: () = msg_send![request, setRecognitionLevel: 0isize];
}

/// The lines of text in a picture, top to bottom and left to right.
/// `languages` in Vision's names, most likely first; empty lets it guess.
pub fn recognize_text(width: u32, height: u32, rgba: Vec<u8>, languages: &[String]) -> Result<Vec<Line>, VisionError> {
    let image = Image::new(width, height, rgba)?;
    autoreleasepool(|_| {
        let request = new_request(c"VNRecognizeTextRequest")?;
        unsafe {
            set_accurate(&request);
            let _: () = msg_send![&*request, setUsesLanguageCorrection: Bool::YES];
            if languages.is_empty() {
                let _: () = msg_send![&*request, setAutomaticallyDetectsLanguage: Bool::YES];
            } else {
                let strings: Vec<Retained<AnyObject>> = languages.iter().map(|l| new_ns_string(l)).collect();
                let array: Retained<AnyObject> = msg_send![class!(NSMutableArray), array];
                for s in &strings {
                    let _: () = msg_send![&*array, addObject: &**s];
                }
                let _: () = msg_send![&*request, setRecognitionLanguages: &*array];
            }
        }
        let handler = handler(&image)?;
        perform(&handler, &request)?;
        let mut lines = Vec::new();
        unsafe {
            let results: Option<Retained<AnyObject>> = msg_send![&*request, results];
            let Some(results) = results else { return Ok(lines) };
            let count: usize = msg_send![&*results, count];
            for i in 0..count {
                let observation: Retained<AnyObject> = msg_send![&*results, objectAtIndex: i];
                let candidates: Option<Retained<AnyObject>> = msg_send![&*observation, topCandidates: 1usize];
                let Some(best) = candidates.and_then(|c| -> Option<Retained<AnyObject>> { msg_send![&*c, firstObject] }) else {
                    continue;
                };
                let text: Option<Retained<AnyObject>> = msg_send![&*best, string];
                let text = text.map(|t| ns_string(&t)).unwrap_or_default();
                if text.trim().is_empty() {
                    continue;
                }
                let r: CGRect = msg_send![&*observation, boundingBox];
                lines.push(Line { text, x: r.origin.x, y: r.origin.y, w: r.size.width, h: r.size.height });
            }
        }
        Ok(reading_order(lines))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_picture_of_the_wrong_size_is_refused() {
        assert!(Image::new(2, 2, vec![0; 3]).is_err());
    }

    #[test]
    fn this_mac_says_what_it_can_do() {
        // Only shape: what a given Mac offers depends on its macOS.
        let a = abilities();
        assert_eq!(a.text, !a.languages.is_empty());
    }

    /// Real pictures: `UTILITIES_RGBA` is raw RGBA with its size in the name,
    /// `name-WxH.rgba`. Writes the mask beside it and prints any text found.
    #[test]
    #[ignore]
    fn real_picture() {
        let path = std::env::var("UTILITIES_RGBA").unwrap();
        let size = path.rsplit('-').next().unwrap().trim_end_matches(".rgba");
        let (w, h) = size.split_once('x').unwrap();
        let (w, h): (u32, u32) = (w.parse().unwrap(), h.parse().unwrap());
        let rgba = std::fs::read(&path).unwrap();
        println!("abilities: {:?}", abilities());
        match subject_mask(w, h, rgba.clone()) {
            Ok(mask) => {
                let covered = mask.iter().filter(|&&v| v > 127).count() as f64 / mask.len() as f64;
                println!("subject covers {:.0}% of the picture", covered * 100.0);
                std::fs::write(format!("{path}.mask"), mask).unwrap();
            }
            Err(e) => println!("subject: {e}"),
        }
        let started = std::time::Instant::now();
        let lines = recognize_text(w, h, rgba, &["vi-VT".into(), "en-US".into()]).unwrap();
        println!("text in {:?}:\n{}", started.elapsed(), super::super::searchable::plain_text(&lines));

        // And a scan made searchable: the picture as a page, the text over it.
        if let Ok(out) = std::env::var("UTILITIES_SCAN_OUT") {
            use crate::utilities::images::{jpegs_to_pdf, read_jpeg, Layout, Orientation, PageSize};
            let jpeg = std::fs::read(std::env::var("UTILITIES_SCAN_JPEG").unwrap()).unwrap();
            let layout = Layout { size: PageSize::Fit, orientation: Orientation::Auto, margin: 0.0 };
            let mut doc = jpegs_to_pdf(&[(read_jpeg(&jpeg).unwrap(), jpeg)], layout, |_, _| true).unwrap();
            let pages = [super::super::searchable::PageLines { page: 1, lines }];
            super::super::searchable::add_text_layer(&mut doc, &pages).unwrap();
            crate::utilities::pdf::save(&mut doc, std::path::Path::new(&out)).unwrap();
        }
    }
}
