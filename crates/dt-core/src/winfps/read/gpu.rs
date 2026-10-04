//! Display adapters present now, and the per-app GPU choice for Deadlock.
//!
//! Adapters come from SetupAPI with `DIGCF_PRESENT`, not from the registry Class key,
//! which keeps entries for GPUs that were removed. It also lists an Optimus dGPU that
//! has no monitor attached, which EnumDisplayDevices would miss.

use windows_sys::Win32::Devices::DeviceAndDriverInstallation::{
    DIGCF_PRESENT, HDEVINFO, SETUP_DI_REGISTRY_PROPERTY, SP_DEVINFO_DATA, SPDRP_DEVICEDESC,
    SPDRP_DRIVER, SPDRP_HARDWAREID, SetupDiDestroyDeviceInfoList, SetupDiEnumDeviceInfo,
    SetupDiGetClassDevsW, SetupDiGetDeviceRegistryPropertyW,
};
use windows_sys::core::GUID;

use super::registry::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, Key};
use crate::winfps::facts::{Gpu, GpuPreference};
use crate::winfps::parse::gpu::{
    is_deadlock_exe, is_software_adapter, parse_driver_date, parse_gpu_preference,
    vendor_from_hardware_id, vram_mib,
};

const GUID_DEVCLASS_DISPLAY: GUID = GUID {
    data1: 0x4d36e968,
    data2: 0xe325,
    data3: 0x11ce,
    data4: [0xbf, 0xc1, 0x08, 0x00, 0x2b, 0xe1, 0x08, 0x10],
};

const INVALID_HDEVINFO: HDEVINFO = -1;

struct DeviceSet(HDEVINFO);

impl Drop for DeviceSet {
    fn drop(&mut self) {
        // SAFETY: the handle came from a successful SetupDiGetClassDevsW and is freed once.
        unsafe { SetupDiDestroyDeviceInfoList(self.0) };
    }
}

/// First string of a registry property (REG_SZ or REG_MULTI_SZ).
fn property(
    set: HDEVINFO,
    dev: &SP_DEVINFO_DATA,
    prop: SETUP_DI_REGISTRY_PROPERTY,
) -> Option<String> {
    let mut buf = [0u16; 1024];
    // SAFETY: `buf` provides `size_of_val` writable bytes; `dev` is initialised by SetupDiEnumDeviceInfo.
    let ok = unsafe {
        SetupDiGetDeviceRegistryPropertyW(
            set,
            dev,
            prop,
            std::ptr::null_mut(),
            buf.as_mut_ptr().cast(),
            std::mem::size_of_val(&buf) as u32,
            std::ptr::null_mut(),
        )
    };
    if ok == 0 {
        return None;
    }
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    let s = String::from_utf16_lossy(&buf[..len]);
    (!s.is_empty()).then_some(s)
}

fn vram(class_key: &Key) -> Option<u64> {
    let bytes = class_key
        .qword("HardwareInformation.qwMemorySize")
        .or_else(|| {
            class_key
                .dword("HardwareInformation.MemorySize")
                .map(u64::from)
        })?;
    vram_mib(bytes)
}

pub fn gpus() -> Vec<Gpu> {
    // SAFETY: the class GUID outlives the call; null enumerator and parent window are allowed.
    let handle = unsafe {
        SetupDiGetClassDevsW(
            &GUID_DEVCLASS_DISPLAY,
            std::ptr::null(),
            std::ptr::null_mut(),
            DIGCF_PRESENT,
        )
    };
    if handle == INVALID_HDEVINFO {
        return Vec::new();
    }
    let set = DeviceSet(handle);
    let mut out = Vec::new();
    for index in 0.. {
        // SAFETY: SP_DEVINFO_DATA is plain data, so all-zero is valid; `cbSize` is set before use.
        let mut dev: SP_DEVINFO_DATA = unsafe { std::mem::zeroed() };
        dev.cbSize = std::mem::size_of::<SP_DEVINFO_DATA>() as u32;
        // SAFETY: `set.0` is a live device set and `dev` is a valid out struct.
        if unsafe { SetupDiEnumDeviceInfo(set.0, index, &mut dev) } == 0 {
            break;
        }
        let Some(name) = property(set.0, &dev, SPDRP_DEVICEDESC) else {
            continue;
        };
        let vendor_id = property(set.0, &dev, SPDRP_HARDWAREID)
            .as_deref()
            .and_then(vendor_from_hardware_id);
        if vendor_id.is_none() || is_software_adapter(&name) {
            continue;
        }
        let class_key = property(set.0, &dev, SPDRP_DRIVER).and_then(|driver| {
            Key::open(
                HKEY_LOCAL_MACHINE,
                &format!(r"SYSTEM\CurrentControlSet\Control\Class\{driver}"),
            )
        });
        out.push(Gpu {
            name,
            vendor_id,
            vram_mib: class_key.as_ref().and_then(vram),
            driver_date: class_key
                .as_ref()
                .and_then(|k| k.string("DriverDate"))
                .as_deref()
                .and_then(parse_driver_date),
            driver_version: class_key.as_ref().and_then(|k| k.string("DriverVersion")),
        });
    }
    out
}

pub fn deadlock_gpu_preference() -> Option<GpuPreference> {
    let key = Key::open(
        HKEY_CURRENT_USER,
        r"Software\Microsoft\DirectX\UserGpuPreferences",
    )?;
    let name = key.value_names().into_iter().find(|n| is_deadlock_exe(n))?;
    parse_gpu_preference(&key.string(&name)?)
}
