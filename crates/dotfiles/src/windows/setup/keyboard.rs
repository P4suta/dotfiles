use anyhow::{Context, Result, bail};

use crate::host_setup_rules;
use crate::windows::{env, proc};
use windows_sys::Win32::UI::Accessibility::FILTERKEYS;
#[cfg(test)]
use windows_sys::Win32::UI::WindowsAndMessaging::{FKF_AVAILABLE, FKF_FILTERKEYSON};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    SPI_GETFILTERKEYS, SPI_SETFILTERKEYS, SPI_SETKEYBOARDDELAY, SPI_SETKEYBOARDSPEED,
    SPIF_SENDCHANGE, SPIF_UPDATEINIFILE, SystemParametersInfoW,
};

/// The filter keys the old setup turned on, which the tests build.
#[cfg(test)]
const LEGACY_FILTER_KEYS_FLAGS: u32 = FKF_FILTERKEYSON | FKF_AVAILABLE;
#[cfg(test)]
const LEGACY_AUTO_REPEAT_DELAY_MS: u32 = 150;
#[cfg(test)]
const LEGACY_AUTO_REPEAT_RATE_MS: u32 = 15;
const ERROR_REQUIRES_INTERACTIVE_WINDOWSTATION: i32 = 1459;
/// The ranges `SPI_SETKEYBOARDDELAY` and `SPI_SETKEYBOARDSPEED` accept.
const MAX_KEYBOARD_DELAY: u32 = 3;
const MAX_KEYBOARD_SPEED: u32 = 31;

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
    if options.keyboard_delay > MAX_KEYBOARD_DELAY {
        bail!("keyboard delay must be between 0 and {MAX_KEYBOARD_DELAY}");
    }
    if options.keyboard_speed > MAX_KEYBOARD_SPEED {
        bail!("keyboard speed must be between 0 and {MAX_KEYBOARD_SPEED}");
    }

    let persist = SPIF_UPDATEINIFILE | SPIF_SENDCHANGE;
    let mut keys = get_filter_keys()?;
    let disable_legacy_filter_keys = disable_managed_filter_keys(&mut keys);
    if let Err(error) = set_value(SPI_SETKEYBOARDDELAY, options.keyboard_delay, persist) {
        if error.raw_os_error() != Some(ERROR_REQUIRES_INTERACTIVE_WINDOWSTATION) {
            bail!("SPI_SETKEYBOARDDELAY: {error}");
        }
        // A remote session has no desktop to notify, and the user profile carries the settings into the next sign-in.
        for (key, value, data) in
            registry_values(options, disable_legacy_filter_keys.then_some(keys.dwFlags))
        {
            let mut cmd = env::command("reg.exe");
            cmd.args(["add", key, "/v", value, "/t", "REG_SZ", "/d", &data, "/f"]);
            proc::capture_ok(&mut cmd).with_context(|| format!("setting {key}\\{value}"))?;
        }
        println!("    no interactive desktop: saved for the next sign-in");
        return Ok(0);
    }
    set_value(SPI_SETKEYBOARDSPEED, options.keyboard_speed, persist)
        .context("SPI_SETKEYBOARDSPEED")?;
    if disable_legacy_filter_keys {
        filter_keys(SPI_SETFILTERKEYS, &mut keys, persist).context("SPI_SETFILTERKEYS")?;
    }
    Ok(0)
}

/// Sets a system parameter whose whole value travels in `value`.
#[expect(
    unsafe_code,
    reason = "SystemParametersInfoW is the only interface to the keyboard settings"
)]
fn set_value(action: u32, value: u32, flags: u32) -> std::io::Result<()> {
    // SAFETY: these actions read only `value` and take no buffer, so nothing dereferences the null pointer.
    let succeeded = unsafe { SystemParametersInfoW(action, value, std::ptr::null_mut(), flags) };
    if succeeded == 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

/// Reads or writes the filter keys settings through `keys`.
#[expect(
    unsafe_code,
    reason = "SystemParametersInfoW is the only interface to the filter keys settings"
)]
fn filter_keys(action: u32, keys: &mut FILTERKEYS, flags: u32) -> std::io::Result<()> {
    // SAFETY: `keys` borrows a live FILTERKEYS mutably, and its cbSize names its own size, as both filter keys actions require.
    let succeeded = unsafe {
        SystemParametersInfoW(action, keys.cbSize, std::ptr::from_mut(keys).cast(), flags)
    };
    if succeeded == 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

/// The per-user registry values `SystemParametersInfoW` would persist for these settings.
fn registry_values(
    options: &Options,
    filter_keys_flags: Option<u32>,
) -> Vec<(&'static str, &'static str, String)> {
    let mut values = vec![
        (
            r"HKCU\Control Panel\Keyboard",
            "KeyboardDelay",
            options.keyboard_delay.to_string(),
        ),
        (
            r"HKCU\Control Panel\Keyboard",
            "KeyboardSpeed",
            options.keyboard_speed.to_string(),
        ),
    ];
    if let Some(flags) = filter_keys_flags {
        values.push((
            r"HKCU\Control Panel\Accessibility\Keyboard Response",
            "Flags",
            flags.to_string(),
        ));
    }
    values
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
    filter_keys(SPI_GETFILTERKEYS, &mut keys, 0).context("SPI_GETFILTERKEYS")?;
    Ok(keys)
}

/// Turns off filter keys that hold exactly the settings the old setup made, as `host_setup_rules` decides, and reports whether it did.
fn disable_managed_filter_keys(keys: &mut FILTERKEYS) -> bool {
    let reset = host_setup_rules::disable_managed_filter_keys(host_setup_rules::FilterKeys {
        flags: keys.dwFlags,
        wait_ms: keys.iWaitMSec,
        delay_ms: keys.iDelayMSec,
        repeat_ms: keys.iRepeatMSec,
        bounce_ms: keys.iBounceMSec,
    });
    if let Some(reset) = reset {
        keys.dwFlags = reset.flags;
    }
    reset.is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_sessions_persist_the_same_settings_in_the_user_profile() {
        let options = Options {
            keyboard_delay: 0,
            keyboard_speed: 31,
        };
        assert_eq!(
            registry_values(&options, None),
            [
                (
                    r"HKCU\Control Panel\Keyboard",
                    "KeyboardDelay",
                    "0".to_owned()
                ),
                (
                    r"HKCU\Control Panel\Keyboard",
                    "KeyboardSpeed",
                    "31".to_owned()
                ),
            ]
        );
        assert_eq!(
            registry_values(&options, Some(FKF_AVAILABLE)).last(),
            Some(&(
                r"HKCU\Control Panel\Accessibility\Keyboard Response",
                "Flags",
                FKF_AVAILABLE.to_string()
            ))
        );
    }
    use windows_sys::Win32::UI::WindowsAndMessaging::FKF_HOTKEYACTIVE;

    #[test]
    fn the_rule_names_the_win32_filter_keys_flags() {
        assert_eq!(host_setup_rules::FILTER_KEYS_ON, FKF_FILTERKEYSON);
        assert_eq!(host_setup_rules::FILTER_KEYS_AVAILABLE, FKF_AVAILABLE);
    }

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
