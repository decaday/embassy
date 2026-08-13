//! SYSCTL configuration for G511x and G518x.

use mspm0_metapac::sysctl::vals;

use crate::sysctl::{ClkOutDiv, div_to_pac};
use crate::pac;

/// Source and configuration for CLK_OUT pin.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum ClkOutSource {
    /// Use SYSOSC as the source.
    ///
    /// The divider is optional for this clock source.
    Sysosc(Option<ClkOutDiv>),

    /// Use ULPCLK as the source.
    ///
    /// The divider is required for this clock source.
    UlpClk(ClkOutDiv),

    /// Use LFCLK as the source.
    ///
    /// The divider is optional for this clock source.
    LfClk(Option<ClkOutDiv>),

    /// Use MFPCLK as the source.
    ///
    /// The divider is required for this clock source.
    MfpClk(ClkOutDiv),

    /// Use HFCLK as the source.
    ///
    /// The divider is optional for this clock source.
    Hfclk(Option<ClkOutDiv>),

    /// Use SYSPLLCLK1 as the source.
    ///
    /// The divider is optional for this clock source.
    SysPllClk1(Option<ClkOutDiv>),

    /// Use USBFLL as the source.
    ///
    /// The divider is required for this clock source.
    UsbFll(ClkOutDiv),
}

impl ClkOutSource {
    pub(super) fn convert_div(self) -> (bool, vals::Exclkdivval) {
        match self {
            ClkOutSource::Sysosc(div) => div_to_pac(div),
            ClkOutSource::UlpClk(div) => div_to_pac(Some(div)),
            ClkOutSource::LfClk(div) => div_to_pac(div),
            ClkOutSource::MfpClk(div) => div_to_pac(Some(div)),
            ClkOutSource::Hfclk(div) => div_to_pac(div),
            ClkOutSource::SysPllClk1(div) => div_to_pac(div),
            ClkOutSource::UsbFll(div) => div_to_pac(Some(div)),
        }
    }

    pub(super) fn convert_src(self) -> vals::Exclksrc {
        match self {
            ClkOutSource::Sysosc(_) => vals::Exclksrc::Sysosc,
            ClkOutSource::UlpClk(_) => vals::Exclksrc::Ulpclk,
            ClkOutSource::LfClk(_) => vals::Exclksrc::Lfclk,
            ClkOutSource::MfpClk(_) => vals::Exclksrc::Mfpclk,
            ClkOutSource::Hfclk(_) => vals::Exclksrc::Hfclk,
            ClkOutSource::SysPllClk1(_) => vals::Exclksrc::Syspllout1,
            ClkOutSource::UsbFll(_) => vals::Exclksrc::Usbfll,
        }
    }
}

/// Reference clock source for the USBFLL.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum UsbFllRef {
    /// USB Start-of-Frame (SOF) signal provided by the host.
    ///
    /// This is the standard choice for USB Device mode: the host drives a 1 ms
    /// SOF frame that the FLL locks on to, producing a stable 48 MHz USBCLK
    /// without requiring any external crystal.
    Sof,

    // /// External 32.768 kHz low-frequency crystal (LFXT).
    // ///
    // /// Allows the USBFLL to lock independently of the USB host.
    // /// Requires LFXT oscillator startup.
    // Not yet implemented.
    // Lfxt,
}

/// Source of the 48 MHz USBCLK supplied to the USB FS peripheral.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum UsbClkSource {
    /// Use the internal USB Frequency Lock Loop (USBFLL).
    ///
    /// The USBFLL locks to the chosen reference and produces 48 MHz USBCLK.
    /// This is the simplest option and works without any external crystal.
    UsbFll(UsbFllRef),

    // /// Use SYSPLL output 1 as USBCLK source.
    // ///
    // /// SYSPLL must be configured to output 48 MHz on CLK1 beforehand.
    // Not yet implemented.
    // SysPll,
}

impl Default for UsbClkSource {
    fn default() -> Self {
        UsbClkSource::UsbFll(UsbFllRef::Sof)
    }
}

/// Configure and start the USB clock source.
///
/// Called from [`crate::init`] before any USB peripheral is powered.
pub(crate) fn configure_usb_clk(source: UsbClkSource) {
    let sysctl = pac::SYSCTL;

    match source {
        UsbClkSource::UsbFll(ref_src) => {
            // Disable USBFLL before configuration
            sysctl.usbfllctl().modify(|w| {
                w.set_fllen(false);
            });
            while sysctl.usbfllctl().read().fllen() {}

            // Configure USBFLL reference source
            sysctl.usbfllctl().modify(|w| {
                w.set_refsel(match ref_src {
                    UsbFllRef::Sof => vals::Refsel::Usbsof,
                    // UsbFllRef::Lfxt => vals::Refsel::Lfclk,
                });
            });

            // Enable USB oscillator clock
            sysctl.usbfllctl().modify(|w| {
                w.set_clken(true);
            });

            // Polling for oscillator ready
            while !sysctl.usbfllstat().read().clkrdy() {}

            // Enable USBFLL
            sysctl.usbfllctl().modify(|w| {
                w.set_fllen(true);
            });

            // Set USBCLK source to USBFLL
            sysctl.genclkcfg().modify(|w| {
                w.set_usbclksrc(vals::Usbclksrc::Usbfllclk);
            });
        }
    }
}
