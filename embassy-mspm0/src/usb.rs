use crate::interrupt::typelevel::Interrupt;
use crate::{interrupt, pac, Peri};
use embassy_hal_internal::PeripheralType;
use core::marker::PhantomData;

/// Interrupt handler.
pub struct InterruptHandler<T: Instance> {
    _phantom: PhantomData<T>,
}

impl<T: Instance> interrupt::typelevel::Handler<T::Interrupt> for InterruptHandler<T> {
    unsafe fn on_interrupt() {
        musb::on_interrupt::<musb::UsbInstance>();
    }
}

pub trait Instance: PeripheralType + 'static {
    type Interrupt: interrupt::typelevel::Interrupt;
}

pub struct UsbDriver<'d, T: Instance> {
    _phantom: PhantomData<&'d mut T>,
}

pub type Driver<'d> = musb::MusbDriver<'d, musb::UsbInstance>;

impl<'d, T: Instance> UsbDriver<'d, T> {
    pub fn new(
        _peri: Peri<'d, T>,
        _irq: impl interrupt::typelevel::Binding<T::Interrupt, InterruptHandler<T>> + 'd,
    ) -> musb::MusbDriver<'d, musb::UsbInstance> {
        let wrapper = pac::USBFS0;
        let sysctl = pac::SYSCTL;

        // Disable USBFLL before configuration
        sysctl.usbfllctl().modify(|w| {
            w.set_fllen(false);
        });
        while sysctl.usbfllctl().read().fllen() {}

        // Configure USBFLL reference source (SOF)
        sysctl.usbfllctl().modify(|w| {
            w.set_refsel(pac::sysctl::vals::Refsel::USBSOF);
        });

        // Enable USB oscillator clock
        sysctl.usbfllctl().modify(|w| {
            w.set_clken(true);
        });

        // Polling for completion
        while !sysctl.usbfllstat().read().clkrdy() {}

        // Enabling USB FLL
        sysctl.usbfllctl().modify(|w| {
            w.set_fllen(true);
        });

        // Set USBCLKSource to USBFLL
        sysctl.genclkcfg().modify(|w| {
            w.set_usbclksrc(pac::sysctl::vals::Usbclksrc::USBFLLCLK);
        });

        // Reset
        wrapper.usbfs0_gprcm(0).usbfs0_rstctl().write(|w| {
            w.0 = 0xB1000003; // KEY + RESETASSERT + RESETSTKYCLR
        });

        wrapper.usbfs0_gprcm(0).usbfs0_rstctl().write(|w| {
            w.0 = 0xB1000000; // KEY + clear resetassert
        });

        // Enable power
        wrapper.usbfs0_gprcm(0).usbfs0_pwren().write(|w| {
            w.0 = 0x26000001; // KEY + ENABLE
        });

        // Wait for power
        while !sysctl.sysstatus().read().usbfs0ready() {}

        // Set Device mode and enable PHY
        wrapper.usbfs0_usbmode().modify(|w| {
            w.set_deviceonly(true);
            w.set_phymode(true);
        });

        // Clear interrupts
        wrapper.usbfs0_cpu_int(0).usbfs0_iclr().write(|w| {
            w.set_intrtx(true);
            w.set_intrrx(true);
            w.set_intrusb(true);
            w.set_vusbpwrdn(true);
        });

        // Enable interrupts
        wrapper.usbfs0_cpu_int(0).usbfs0_imask().write(|w| {
            w.set_intrtx(true);
            w.set_intrrx(true);
            w.set_intrusb(true);
        });

        T::Interrupt::unpend();
        unsafe { T::Interrupt::enable() };

        musb::MusbDriver::new()
    }
}

impl Instance for crate::peripherals::USBFS0 {
    type Interrupt = crate::interrupt::typelevel::USBFS0;
}
