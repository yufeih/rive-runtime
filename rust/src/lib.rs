use anyhow::Result;
use std::{
    ffi::{CString, c_void},
    io, ptr,
    sync::Mutex,
    time::Duration,
};
use windows::{
    Win32::{Foundation::HWND, UI::HiDpi::GetDpiForWindow},
    core::HRESULT,
};

static RUNTIME: Mutex<()> = Mutex::new(());

type Handle = *mut c_void;

macro_rules! api {
    ($($field:ident: unsafe extern "C" fn($($arg:ty),*) $(-> $ret:ty)? = $name:literal),* $(,)?) => {
        struct Api {
            $($field: unsafe extern "C" fn($($arg),*) $(-> $ret)?,)*
        }

        mod native {
            use super::*;
            unsafe extern "C" {
                $(#[link_name = $name]
                pub fn $field($(_: $arg),*) $(-> $ret)?;)*
            }
        }

        impl Api {
            fn new() -> Self {
                Self { $($field: native::$field,)* }
            }
        }
    };
}

api! {
    load: unsafe extern "C" fn(*const u8, u64, Handle, Handle, Handle) -> Handle = "loadRiveFile",
    artboard: unsafe extern "C" fn(Handle, bool) -> Handle = "riveFileArtboardDefault",
    machine: unsafe extern "C" fn(Handle) -> Handle = "riveArtboardStateMachineDefault",
    model: unsafe extern "C" fn(Handle, Handle) -> Handle = "riveFileDefaultArtboardViewModelRuntime",
    instance: unsafe extern "C" fn(Handle) -> Handle = "createDefaultVMIRuntime",
    bind: unsafe extern "C" fn(Handle, Handle) = "stateMachineSetVMIRuntime",
    number: unsafe extern "C" fn(Handle, *const i8) -> Handle = "vmiRuntimeGetNumberProperty",
    boolean: unsafe extern "C" fn(Handle, *const i8) -> Handle = "vmiRuntimeGetBooleanProperty",
    string: unsafe extern "C" fn(Handle, *const i8) -> Handle = "vmiRuntimeGetStringProperty",
    list: unsafe extern "C" fn(Handle, *const i8) -> Handle = "vmiRuntimeGetListProperty",
    list_size: unsafe extern "C" fn(Handle) -> usize = "getVMIListRuntimeSize",
    list_item: unsafe extern "C" fn(Handle, i32) -> Handle = "vmiListRuntimeInstanceAt",
    set_number: unsafe extern "C" fn(Handle, f32) = "setVMINumberRuntimeValue",
    get_number: unsafe extern "C" fn(Handle) -> f32 = "getVMINumberRuntimeValue",
    set_boolean: unsafe extern "C" fn(Handle, bool) = "setVMIBooleanRuntimeValue",
    set_string: unsafe extern "C" fn(Handle, *const i8) = "setVMIStringRuntimeValue",
    advance: unsafe extern "C" fn(Handle, f32) -> bool = "stateMachineInstanceAdvanceAndApply",
    width: unsafe extern "C" fn(Handle) -> f32 = "riveArtboardGetOriginalWidth",
    height: unsafe extern "C" fn(Handle) -> f32 = "riveArtboardGetOriginalHeight",
    save: unsafe extern "C" fn(Handle) = "save",
    restore: unsafe extern "C" fn(Handle) = "restore",
    transform: unsafe extern "C" fn(Handle, f32, f32, f32, f32, f32, f32) = "transform",
    draw: unsafe extern "C" fn(Handle, Handle) = "artboardDraw",
    delete_value: unsafe extern "C" fn(Handle) = "deleteVMIValueRuntime",
    delete_instance: unsafe extern "C" fn(Handle) = "deleteVMIRuntime",
    delete_model: unsafe extern "C" fn(Handle) = "deleteViewModelRuntime",
    delete_machine: unsafe extern "C" fn(Handle) = "deleteStateMachineInstance",
    delete_artboard: unsafe extern "C" fn(Handle) = "deleteArtboardInstance",
    delete_file: unsafe extern "C" fn(Handle) = "deleteRiveFile",
}

pub struct Renderer {
    api: Api,
    surface: Handle,
    file: Handle,
    artboard: Handle,
    machine: Handle,
    model: Handle,
    instance: Handle,
    dimensions: [f32; 2],
    size: [u32; 2],
    window: HWND,
}

unsafe extern "C" {
    fn rive_audio_set_muted(muted: bool) -> HRESULT;
    fn rive_surface_create(window: HWND, width: u32, height: u32, output: *mut Handle) -> HRESULT;
    fn rive_surface_destroy(surface: Handle);
    fn rive_surface_resize(surface: Handle, width: u32, height: u32) -> HRESULT;
    fn rive_surface_factory(surface: Handle) -> Handle;
    fn rive_surface_begin(surface: Handle) -> Handle;
    fn rive_window_size(window: HWND, width: *mut u32, height: *mut u32) -> HRESULT;
    fn rive_surface_present(surface: Handle, scale: f32) -> HRESULT;
    fn rive_surface_read_rgba(surface: Handle, pixels: *mut u8, length: usize) -> HRESULT;
}

pub fn set_audio_enabled(enabled: bool) -> Result<()> {
    let _guard = RUNTIME.lock().unwrap();
    unsafe {
        rive_audio_set_muted(!enabled).ok()?;
    }
    Ok(())
}

fn required(handle: Handle) -> Result<Handle> {
    match handle.is_null() {
        true => Err(io::Error::from(io::ErrorKind::InvalidData).into()),
        false => Ok(handle),
    }
}

impl Renderer {
    pub fn for_window(bytes: &[u8], window: HWND) -> Result<Self> {
        let mut size = [0; 2];
        unsafe {
            rive_window_size(window, &mut size[0], &mut size[1]).ok()?;
        }
        Self::new(bytes, window, size)
    }

    pub fn offscreen(bytes: &[u8]) -> Result<Self> {
        let mut renderer = Self::new(bytes, HWND::default(), [1, 1])?;
        renderer.resize(renderer.dimensions.map(|value| value.ceil() as u32))?;
        Ok(renderer)
    }

    pub fn dimensions(&self) -> [f32; 2] {
        self.dimensions
    }

    pub fn set_number(&mut self, name: &str, value: f32) -> Result<bool> {
        if !value.is_finite() {
            return Err(io::Error::from(io::ErrorKind::InvalidInput).into());
        }
        let name = CString::new(name)?;
        let _guard = RUNTIME.lock().unwrap();
        unsafe {
            let property = (self.api.number)(self.instance, name.as_ptr());
            if property.is_null() {
                return Ok(false);
            }
            (self.api.set_number)(property, value);
            (self.api.delete_value)(property);
        }
        Ok(true)
    }

    pub fn set_bool(&mut self, name: &str, value: bool) -> Result<bool> {
        let name = CString::new(name)?;
        let _guard = RUNTIME.lock().unwrap();
        unsafe {
            let property = (self.api.boolean)(self.instance, name.as_ptr());
            if property.is_null() {
                return Ok(false);
            }
            (self.api.set_boolean)(property, value);
            (self.api.delete_value)(property);
        }
        Ok(true)
    }

    pub fn set_string(&mut self, name: &str, value: &str) -> Result<bool> {
        let name = CString::new(name)?;
        let value = CString::new(value)?;
        let _guard = RUNTIME.lock().unwrap();
        unsafe {
            let property = (self.api.string)(self.instance, name.as_ptr());
            if property.is_null() {
                return Ok(false);
            }
            (self.api.set_string)(property, value.as_ptr());
            (self.api.delete_value)(property);
        }
        Ok(true)
    }

    pub fn number_list(&self, name: &str, field: &str) -> Result<Vec<f32>> {
        let name = CString::new(name)?;
        let field = CString::new(field)?;
        let _guard = RUNTIME.lock().unwrap();
        unsafe {
            let list = (self.api.list)(self.instance, name.as_ptr());
            if list.is_null() {
                return Ok(Vec::new());
            }
            let count = (self.api.list_size)(list);
            let result = (|| -> Result<Vec<f32>> {
                let mut values = Vec::with_capacity(count);
                for index in 0..count {
                    let instance = required((self.api.list_item)(list, i32::try_from(index)?))?;
                    let property = (self.api.number)(instance, field.as_ptr());
                    (self.api.delete_instance)(instance);
                    let property = required(property)?;
                    values.push((self.api.get_number)(property));
                    (self.api.delete_value)(property);
                }
                Ok(values)
            })();
            (self.api.delete_value)(list);
            result
        }
    }

    pub fn set_number_list(&mut self, name: &str, field: &str, values: &[f32]) -> Result<bool> {
        if values.iter().any(|v| !v.is_finite()) {
            return Err(io::Error::from(io::ErrorKind::InvalidInput).into());
        }
        let name = CString::new(name)?;
        let field = CString::new(field)?;
        let _guard = RUNTIME.lock().unwrap();
        unsafe {
            let list = (self.api.list)(self.instance, name.as_ptr());
            if list.is_null() {
                return Ok(false);
            }
            let count = (self.api.list_size)(list);
            let result = (|| -> Result<()> {
                if count != values.len() {
                    return Err(io::Error::from(io::ErrorKind::InvalidInput).into());
                }
                for (index, &value) in values.iter().enumerate() {
                    let instance = required((self.api.list_item)(list, i32::try_from(index)?))?;
                    let property = (self.api.number)(instance, field.as_ptr());
                    (self.api.delete_instance)(instance);
                    let property = required(property)?;
                    (self.api.set_number)(property, value);
                    (self.api.delete_value)(property);
                }
                Ok(())
            })();
            (self.api.delete_value)(list);
            result?;
        }
        Ok(true)
    }

    pub fn present(&mut self, elapsed: Duration) -> Result<()> {
        let mut size = [0; 2];
        unsafe {
            rive_window_size(self.window, &mut size[0], &mut size[1]).ok()?;
        }
        self.resize(size)?;
        let _guard = RUNTIME.lock().unwrap();
        let scale = self.draw(elapsed)?;
        unsafe {
            rive_surface_present(self.surface, scale).ok()?;
        }
        Ok(())
    }

    pub fn advance(&mut self, elapsed: Duration) {
        let _guard = RUNTIME.lock().unwrap();
        unsafe {
            (self.api.advance)(self.machine, elapsed.as_secs_f32());
        }
    }

    pub fn screenshot(&mut self) -> Result<Vec<u8>> {
        let mut pixels = vec![0; self.size[0] as usize * self.size[1] as usize * 4];
        let _guard = RUNTIME.lock().unwrap();
        self.draw(Duration::ZERO)?;
        unsafe {
            rive_surface_read_rgba(self.surface, pixels.as_mut_ptr(), pixels.len()).ok()?;
        }
        for pixel in pixels.as_chunks_mut::<4>().0 {
            let alpha = u32::from(pixel[3]);
            for channel in &mut pixel[..3] {
                *channel = match (u32::from(*channel) * 255 + alpha / 2).checked_div(alpha) {
                    Some(value) => value.min(255) as u8,
                    None => 0,
                };
            }
        }
        Ok(pixels)
    }

    fn new(bytes: &[u8], window: HWND, size: [u32; 2]) -> Result<Self> {
        if bytes.is_empty() || size.contains(&0) || size.iter().any(|&v| v > 16384) {
            return Err(io::Error::from(io::ErrorKind::InvalidInput).into());
        }
        let mut renderer = Self {
            api: Api::new(),
            surface: ptr::null_mut(),
            file: ptr::null_mut(),
            artboard: ptr::null_mut(),
            machine: ptr::null_mut(),
            model: ptr::null_mut(),
            instance: ptr::null_mut(),
            dimensions: [0.0; 2],
            size,
            window,
        };
        {
            let _guard = RUNTIME.lock().unwrap();
            unsafe {
                rive_surface_create(window, size[0], size[1], &mut renderer.surface).ok()?;
                let factory = required(rive_surface_factory(renderer.surface))?;
                renderer.file = required((renderer.api.load)(
                    bytes.as_ptr(),
                    bytes.len() as u64,
                    factory,
                    ptr::null_mut(),
                    ptr::null_mut(),
                ))?;
                renderer.artboard = required((renderer.api.artboard)(renderer.file, false))?;
                renderer.machine = required((renderer.api.machine)(renderer.artboard))?;
                renderer.model = (renderer.api.model)(renderer.file, renderer.artboard);
                if !renderer.model.is_null() {
                    renderer.instance = required((renderer.api.instance)(renderer.model))?;
                    (renderer.api.bind)(renderer.machine, renderer.instance);
                }
                renderer.dimensions = [
                    (renderer.api.width)(renderer.artboard),
                    (renderer.api.height)(renderer.artboard),
                ];
                if renderer
                    .dimensions
                    .iter()
                    .any(|v| !v.is_finite() || *v <= 0.0)
                {
                    return Err(io::Error::from(io::ErrorKind::InvalidData).into());
                }
            }
        }
        Ok(renderer)
    }

    fn resize(&mut self, size: [u32; 2]) -> Result<()> {
        if size.contains(&0) || size.iter().any(|&v| v > 16384) {
            return Err(io::Error::from(io::ErrorKind::InvalidInput).into());
        }
        if self.size != size {
            let _guard = RUNTIME.lock().unwrap();
            unsafe {
                rive_surface_resize(self.surface, size[0], size[1]).ok()?;
            }
            self.size = size;
        }
        Ok(())
    }

    fn draw(&mut self, elapsed: Duration) -> Result<f32> {
        unsafe {
            let max_scale = match self.window.is_invalid() {
                true => f32::INFINITY,
                false => {
                    let dpi = GetDpiForWindow(self.window);
                    if dpi == 0 {
                        return Err(io::Error::last_os_error().into());
                    }
                    dpi as f32 / 96.0
                }
            };
            (self.api.advance)(self.machine, elapsed.as_secs_f32());
            let native = required(rive_surface_begin(self.surface))?;
            let scale = (self.size[0] as f32 / self.dimensions[0])
                .min(self.size[1] as f32 / self.dimensions[1])
                .min(max_scale);
            (self.api.save)(native);
            (self.api.transform)(
                native,
                scale,
                0.0,
                0.0,
                scale,
                ((self.size[0] as f32 - self.dimensions[0] * scale) * 0.5).round(),
                ((self.size[1] as f32 - self.dimensions[1] * scale) * 0.5).round(),
            );
            (self.api.draw)(self.artboard, native);
            (self.api.restore)(native);
            Ok(scale)
        }
    }
}

impl Drop for Renderer {
    fn drop(&mut self) {
        let _guard = RUNTIME.lock().unwrap();
        unsafe {
            for (handle, delete) in [
                (self.machine, self.api.delete_machine),
                (self.artboard, self.api.delete_artboard),
                (self.instance, self.api.delete_instance),
                (self.model, self.api.delete_model),
                (self.file, self.api.delete_file),
            ] {
                if !handle.is_null() {
                    delete(handle);
                }
            }
            if !self.surface.is_null() {
                rive_surface_destroy(self.surface);
            }
        }
    }
}

unsafe impl Send for Renderer {}
