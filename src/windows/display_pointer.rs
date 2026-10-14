//! The pointer shape Windows hands to the display, read with DXGI Desktop
//! Duplication. GDI screen captures never contain a hardware (GPU-drawn)
//! pointer, so only this shows what the cursor feedback looks like on screen.
use super::*;
use std::time::Duration;
use windows::{
    Win32::{
        Foundation::HMODULE,
        Graphics::{
            Direct3D::D3D_DRIVER_TYPE_HARDWARE,
            Direct3D11::{D3D11_CREATE_DEVICE_FLAG, D3D11_SDK_VERSION, D3D11CreateDevice},
            Dxgi::*,
        },
    },
    core::Interface,
};

/// One pointer shape update. Colour shapes keep their BGRA pixels.
#[derive(Clone, PartialEq)]
pub(super) struct PointerShape {
    pub kind: u32,
    pub width: u32,
    pub height: u32,
    pub hotspot: (i32, i32),
    pub pixels: Vec<u32>,
}
impl PointerShape {
    pub fn kind_name(&self) -> &'static str {
        match self.kind {
            1 => "monochrome",
            2 => "color",
            4 => "masked-color",
            _ => "unknown",
        }
    }
    /// Opaque pixels that are black and that are toolbar yellow.
    pub fn black_and_yellow(&self) -> (usize, usize) {
        let [yellow_r, yellow_g, _] = drawing::CONTRAST_YELLOW.map(u32::from);
        let opaque = self.pixels.iter().filter(|pixel| match self.kind {
            2 => *pixel >> 24 >= 250,
            // Masked colour: alpha 0 replaces the screen pixel, 0xFF XORs it.
            4 => *pixel >> 24 == 0,
            _ => false,
        });
        let channel = |pixel: u32, shift: u32| (pixel >> shift) & 255;
        opaque.fold((0, 0), |(black, yellow), pixel| {
            let (r, g, b) = (channel(*pixel, 16), channel(*pixel, 8), channel(*pixel, 0));
            (
                black + usize::from(r <= 24 && g <= 24 && b <= 24),
                yellow + usize::from(r + 16 >= yellow_r && g + 16 >= yellow_g && b <= 48),
            )
        })
    }
    pub fn report(&self) -> serde_json::Value {
        let (black, yellow) = self.black_and_yellow();
        serde_json::json!({"type":self.kind_name(),"width":self.width,"height":self.height,
            "hotspot":[self.hotspot.0,self.hotspot.1],"black_pixels":black,"yellow_pixels":yellow})
    }
}

pub(super) struct Duplication {
    duplication: IDXGIOutputDuplication,
    pub latest: Option<PointerShape>,
}
impl Duplication {
    /// Duplicates the output showing the pointer. Requires an interactive
    /// desktop and a per-monitor DPI aware process.
    pub unsafe fn new() -> Result<Self, String> {
        let error = |step: &str, e: windows::core::Error| format!("{step}: {e}");
        let mut device = None;
        D3D11CreateDevice(
            None,
            D3D_DRIVER_TYPE_HARDWARE,
            HMODULE::default(),
            D3D11_CREATE_DEVICE_FLAG(0),
            None,
            D3D11_SDK_VERSION,
            Some(&mut device),
            None,
            None,
        )
        .map_err(|e| error("D3D11CreateDevice", e))?;
        let device = device.ok_or("D3D11CreateDevice returned no device")?;
        let adapter = device
            .cast::<IDXGIDevice>()
            .and_then(|device| device.GetAdapter())
            .map_err(|e| error("GetAdapter", e))?;
        let (x, y) = cursor();
        let mut index = 0;
        let output = loop {
            let output = adapter
                .EnumOutputs(index)
                .map_err(|e| error("No output contains the pointer", e))?;
            let r = output
                .GetDesc()
                .map_err(|e| error("GetDesc", e))?
                .DesktopCoordinates;
            if (r.left..r.right).contains(&x) && (r.top..r.bottom).contains(&y) {
                break output;
            }
            index += 1;
        };
        let duplication = output
            .cast::<IDXGIOutput1>()
            .and_then(|output| output.DuplicateOutput(&device))
            .map_err(|e| error("DuplicateOutput", e))?;
        Ok(Self {
            duplication,
            latest: None,
        })
    }
    /// Receive frames for up to `wait`, keeping the latest pointer shape.
    /// Returns whether a new shape arrived.
    pub unsafe fn poll(&mut self, wait: Duration) -> bool {
        let deadline = Instant::now() + wait;
        let mut updated = false;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            let mut info = DXGI_OUTDUPL_FRAME_INFO::default();
            let mut resource = None;
            if self
                .duplication
                .AcquireNextFrame(remaining.as_millis() as u32, &mut info, &mut resource)
                .is_ok()
            {
                if info.PointerShapeBufferSize > 0 {
                    let mut buffer = vec![0u8; info.PointerShapeBufferSize as usize];
                    let mut required = 0;
                    let mut shape = DXGI_OUTDUPL_POINTER_SHAPE_INFO::default();
                    if self
                        .duplication
                        .GetFramePointerShape(
                            buffer.len() as u32,
                            buffer.as_mut_ptr().cast(),
                            &mut required,
                            &mut shape,
                        )
                        .is_ok()
                    {
                        self.latest = Some(decode(&buffer, &shape));
                        updated = true;
                    }
                }
                drop(resource);
                let _ = self.duplication.ReleaseFrame();
            }
            if updated || remaining.is_zero() {
                return updated;
            }
        }
    }
    /// Wait until the pointer shape differs from `previous`.
    pub unsafe fn wait_for_change(
        &mut self,
        previous: Option<&PointerShape>,
        wait: Duration,
    ) -> bool {
        let deadline = Instant::now() + wait;
        while Instant::now() < deadline {
            self.poll(deadline.saturating_duration_since(Instant::now()));
            if self.latest.as_ref() != previous {
                return true;
            }
        }
        false
    }
}

fn decode(buffer: &[u8], shape: &DXGI_OUTDUPL_POINTER_SHAPE_INFO) -> PointerShape {
    let (width, pitch) = (shape.Width as usize, shape.Pitch as usize);
    // Monochrome shapes stack their AND and XOR masks vertically.
    let height = if shape.Type == 1 {
        shape.Height / 2
    } else {
        shape.Height
    } as usize;
    let pixels = if shape.Type == 1 {
        Vec::new()
    } else {
        (0..height)
            .flat_map(|y| {
                (0..width).map(move |x| {
                    let at = y * pitch + x * 4;
                    buffer
                        .get(at..at + 4)
                        .map_or(0, |bytes| u32::from_le_bytes(bytes.try_into().unwrap()))
                })
            })
            .collect()
    };
    PointerShape {
        kind: shape.Type,
        width: shape.Width,
        height: height as u32,
        hotspot: (shape.HotSpot.x, shape.HotSpot.y),
        pixels,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colour_shapes_count_black_and_toolbar_yellow() {
        let shape = PointerShape {
            kind: 2,
            width: 4,
            height: 1,
            hotspot: (0, 0),
            pixels: vec![0xff000000, 0xffffe600, 0x00ffe600, 0xffbfff00],
        };
        assert_eq!(shape.black_and_yellow(), (1, 1));
        let masked = PointerShape { kind: 4, ..shape };
        assert_eq!(masked.black_and_yellow(), (0, 1));
    }
}
