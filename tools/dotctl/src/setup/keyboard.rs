#![allow(unsafe_code)]

use anyhow::{Result, bail};
use windows_sys::Win32::UI::Accessibility::FILTERKEYS;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    FKF_AVAILABLE, FKF_FILTERKEYSON, SPI_GETFILTERKEYS, SPI_SETFILTERKEYS, SPI_SETKEYBOARDDELAY,
    SPI_SETKEYBOARDSPEED, SPIF_SENDCHANGE, SPIF_UPDATEINIFILE, SystemParametersInfoW,
};

const LEGACY_FILTER_KEYS_FLAGS: u32 = FKF_FILTERKEYSON | FKF_AVAILABLE;
const LEGACY_AUTO_REPEAT_DELAY_MS: u32 = 150;
const LEGACY_AUTO_REPEAT_RATE_MS: u32 = 15;

#[derive(Debug)]
pub struct Options {
    pub keyboard_delay: u32,
    pub keyboard_speed: u32,
}

pub fn run(options: &Options) -> Result<i32> {
    println!(
        ">>> keyboard delay {}, repeat speed {}",
        options.keyboard_delay, options.keyboard_speed
    );
    if options.keyboard_delay > 3 {
        bail!("keyboard delay must be between 0 and 3");
    }
    if options.keyboard_speed > 31 {
        bail!("keyboard speed must be between 0 and 31");
    }

    let persist = SPIF_UPDATEINIFILE | SPIF_SENDCHANGE;
    let mut keys = get_filter_keys()?;
    let disable_legacy_filter_keys = disable_managed_filter_keys(&mut keys);
    unsafe {
        if SystemParametersInfoW(
            SPI_SETKEYBOARDDELAY,
            options.keyboard_delay,
            std::ptr::null_mut(),
            persist,
        ) == 0
        {
            bail!("SPI_SETKEYBOARDDELAY: {}", std::io::Error::last_os_error());
        }
        if SystemParametersInfoW(
            SPI_SETKEYBOARDSPEED,
            options.keyboard_speed,
            std::ptr::null_mut(),
            persist,
        ) == 0
        {
            bail!("SPI_SETKEYBOARDSPEED: {}", std::io::Error::last_os_error());
        }
        if disable_legacy_filter_keys
            && SystemParametersInfoW(
                SPI_SETFILTERKEYS,
                keys.cbSize,
                (&raw mut keys).cast(),
                persist,
            ) == 0
        {
            bail!("SPI_SETFILTERKEYS: {}", std::io::Error::last_os_error());
        }
    }
    Ok(0)
}

fn get_filter_keys() -> Result<FILTERKEYS> {
    let mut keys = FILTERKEYS {
        cbSize: u32::try_from(std::mem::size_of::<FILTERKEYS>()).unwrap_or_default(),
        dwFlags: 0,
        iWaitMSec: 0,
        iDelayMSec: 0,
        iRepeatMSec: 0,
        iBounceMSec: 0,
    };
    unsafe {
        if SystemParametersInfoW(SPI_GETFILTERKEYS, keys.cbSize, (&raw mut keys).cast(), 0) == 0 {
            bail!("SPI_GETFILTERKEYS: {}", std::io::Error::last_os_error());
        }
    }
    Ok(keys)
}

fn disable_managed_filter_keys(keys: &mut FILTERKEYS) -> bool {
    let is_managed = keys.dwFlags == LEGACY_FILTER_KEYS_FLAGS
        && keys.iWaitMSec == 0
        && keys.iDelayMSec == LEGACY_AUTO_REPEAT_DELAY_MS
        && keys.iRepeatMSec == LEGACY_AUTO_REPEAT_RATE_MS
        && keys.iBounceMSec == 0;
    if is_managed {
        keys.dwFlags &= !FKF_FILTERKEYSON;
    }
    is_managed
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows_sys::Win32::UI::WindowsAndMessaging::FKF_HOTKEYACTIVE;

    fn legacy_filter_keys() -> FILTERKEYS {
        FILTERKEYS {
            cbSize: u32::try_from(std::mem::size_of::<FILTERKEYS>()).unwrap_or_default(),
            dwFlags: LEGACY_FILTER_KEYS_FLAGS,
            iWaitMSec: 0,
            iDelayMSec: LEGACY_AUTO_REPEAT_DELAY_MS,
            iRepeatMSec: LEGACY_AUTO_REPEAT_RATE_MS,
            iBounceMSec: 0,
        }
    }

    fn snapshot(keys: &FILTERKEYS) -> (u32, u32, u32, u32, u32) {
        (
            keys.dwFlags,
            keys.iWaitMSec,
            keys.iDelayMSec,
            keys.iRepeatMSec,
            keys.iBounceMSec,
        )
    }

    #[test]
    fn managed_filter_keys_are_disabled() {
        let mut keys = legacy_filter_keys();
        assert!(disable_managed_filter_keys(&mut keys));
        assert_eq!(keys.dwFlags & FKF_FILTERKEYSON, 0);
        assert_eq!(keys.dwFlags & FKF_AVAILABLE, FKF_AVAILABLE);
        assert_eq!(keys.dwFlags & FKF_HOTKEYACTIVE, 0);
        assert_eq!(
            snapshot(&keys),
            (
                FKF_AVAILABLE,
                0,
                LEGACY_AUTO_REPEAT_DELAY_MS,
                LEGACY_AUTO_REPEAT_RATE_MS,
                0,
            )
        );
    }

    #[test]
    fn disabled_filter_keys_are_preserved() {
        let mut keys = legacy_filter_keys();
        keys.dwFlags &= !FKF_FILTERKEYSON;
        let before = snapshot(&keys);
        assert!(!disable_managed_filter_keys(&mut keys));
        assert_eq!(snapshot(&keys), before);
    }

    #[test]
    fn custom_filter_keys_are_preserved() {
        let mut keys = legacy_filter_keys();
        keys.iDelayMSec = 500;
        let before = snapshot(&keys);
        assert!(!disable_managed_filter_keys(&mut keys));
        assert_eq!(snapshot(&keys), before);
    }
}
