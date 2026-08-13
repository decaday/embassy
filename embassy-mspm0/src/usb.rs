use crate::interrupt::typelevel::Interrupt;
use crate::{Peri, interrupt, pac};
use embassy_hal_internal::PeripheralType;

/// Interrupt handler.
pub struct InterruptHandler<T: Instance> {
    _phantom: core::marker::PhantomData<T>,
}

impl<T: Instance> interrupt::typelevel::Handler<T::Interrupt> for InterruptHandler<T> {
    unsafe fn on_interrupt() {
        musb::on_interrupt::<musb::UsbInstance>();
    }
}

pub trait Instance: PeripheralType + 'static {
    type Interrupt: interrupt::typelevel::Interrupt;
}

pub struct Driver<'d, T: Instance> {
    inner: musb::MusbDriver<'d, musb::UsbInstance>,
    _phantom: core::marker::PhantomData<&'d mut T>,
}

/// USB driver.
impl<'d, T: Instance> Driver<'d, T> {
    pub fn new(
        _peri: Peri<'d, T>,
        _irq: impl interrupt::typelevel::Binding<T::Interrupt, InterruptHandler<T>> + 'd,
    ) -> Self {
        let wrapper = pac::USBFS0;

        // Reset
        wrapper.gprcm(0).rstctl().write(|w| {
            w.set_key(pac::usbfs::vals::ResetKey::Key);
            w.set_resetassert(true);
            w.set_resetstkyclr(true);
        });

        wrapper.gprcm(0).rstctl().write(|w| {
            w.set_key(pac::usbfs::vals::ResetKey::Key);
            w.set_resetassert(false);
        });

        // Enable power
        wrapper.gprcm(0).pwren().write(|w| {
            w.set_key(pac::usbfs::vals::PwrenKey::Key);
            w.set_enable(true);
        });

        // Wait for power
        while !pac::SYSCTL.sysstatus().read().usbfs0ready() {}

        // Set Device mode and enable PHY
        // Note: Currently we only consider device mode.
        wrapper.usbmode().modify(|w| {
            w.set_deviceonly(true);
            w.set_phymode(true); // PHYMODE was reduced to a boolean by chiptool
        });

        // Clear interrupts
        wrapper.cpu_int(0).iclr().write(|w| {
            w.set_intrtx(true);
            w.set_intrrx(true);
            w.set_intrusb(true);
            w.set_vusbpwrdn(true);
        });

        // Enable interrupts
        wrapper.cpu_int(0).imask().write(|w| {
            w.set_intrtx(true);
            w.set_intrrx(true);
            w.set_intrusb(true);
        });

        T::Interrupt::unpend();
        unsafe { T::Interrupt::enable() };

        Self {
            inner: musb::MusbDriver::new(),
            _phantom: core::marker::PhantomData,
        }
    }
}

impl<'d, T: Instance> embassy_usb_driver::Driver<'d> for Driver<'d, T> {
    type EndpointOut = musb::Endpoint<'d, musb::UsbInstance, musb::Out>;
    type EndpointIn = musb::Endpoint<'d, musb::UsbInstance, musb::In>;
    type ControlPipe = musb::ControlPipe<'d, musb::UsbInstance>;
    type Bus = musb::Bus<'d, musb::UsbInstance>;

    fn alloc_endpoint_in(
        &mut self,
        ep_type: embassy_usb_driver::EndpointType,
        ep_addr: Option<embassy_usb_driver::EndpointAddress>,
        max_packet_size: u16,
        interval_ms: u8,
    ) -> Result<Self::EndpointIn, embassy_usb_driver::EndpointAllocError> {
        embassy_usb_driver::Driver::alloc_endpoint_in(
            &mut self.inner,
            ep_type,
            ep_addr,
            max_packet_size,
            interval_ms,
        )
    }

    fn alloc_endpoint_out(
        &mut self,
        ep_type: embassy_usb_driver::EndpointType,
        ep_addr: Option<embassy_usb_driver::EndpointAddress>,
        max_packet_size: u16,
        interval_ms: u8,
    ) -> Result<Self::EndpointOut, embassy_usb_driver::EndpointAllocError> {
        embassy_usb_driver::Driver::alloc_endpoint_out(
            &mut self.inner,
            ep_type,
            ep_addr,
            max_packet_size,
            interval_ms,
        )
    }

    fn start(self, control_max_packet_size: u16) -> (Self::Bus, Self::ControlPipe) {
        embassy_usb_driver::Driver::start(self.inner, control_max_packet_size)
    }
}

impl Instance for crate::peripherals::USBFS0 {
    type Interrupt = crate::interrupt::typelevel::USBFS0;
}
