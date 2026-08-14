#![no_std]
#![no_main]

use core::sync::atomic::{AtomicU8, Ordering};

use defmt::{info, panic, warn};
use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_futures::join::join3;
use embassy_mspm0::bind_interrupts;
use embassy_mspm0::gpio::{Input, Pull};
use embassy_mspm0::peripherals::USBFS0;
use embassy_mspm0::usb::{InterruptHandler, Driver};
use embassy_usb::Builder;
use embassy_usb::class::cdc_acm::{CdcAcmClass, State as CdcAcmState};
use embassy_usb::class::hid::{
    HidBootProtocol, HidProtocolMode, HidSubclass, HidWriter, ReportId, RequestHandler, State as HidState,
};
use embassy_usb::control::OutResponse;
use embassy_usb::driver::EndpointError;
use panic_probe as _;
use usbd_hid::descriptor::{KeyboardReport, SerializedDescriptor};

bind_interrupts!(struct Irqs {
    USBFS0 => InterruptHandler<USBFS0>;
});

static HID_PROTOCOL_MODE: AtomicU8 = AtomicU8::new(HidProtocolMode::Boot as u8);

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    info!("Hello there!");

    let config = embassy_mspm0::Config::default();
    let p = embassy_mspm0::init(config);

    // Create the driver, from the HAL.
    let driver = Driver::new(p.USBFS0, Irqs);

    // Create embassy-usb Config
    let mut config = embassy_usb::Config::new(0xc0de, 0xcafe);
    config.manufacturer = Some("Embassy");
    config.product = Some("USB-composite example");
    config.serial_number = Some("12345678");
    config.max_power = 100;
    config.max_packet_size_0 = 64;
    // Composite requirement: device_class = 0xEF, device_sub_class = 0x02, device_protocol = 0x01
    // config.device_class = 0xEF;
    // config.device_sub_class = 0x02;
    // config.device_protocol = 0x01;
    config.composite_with_iads = true;

    // Create embassy-usb DeviceBuilder using the driver and config.
    let mut config_descriptor = [0; 256];
    let mut bos_descriptor = [0; 256];
    let mut control_buf = [0; 64];

    let mut request_handler = MyRequestHandler {};
    let mut hid_state = HidState::new();
    let mut cdc_state = CdcAcmState::new();

    let mut builder = Builder::new(
        driver,
        config,
        &mut config_descriptor,
        &mut bos_descriptor,
        &mut [], // no msos descriptors
        &mut control_buf,
    );

    // Create classes on the builder.
    // 1. CDC ACM
    let mut class = CdcAcmClass::new(&mut builder, &mut cdc_state, 64);

    // 2. HID Keyboard
    let hid_config = embassy_usb::class::hid::Config {
        report_descriptor: KeyboardReport::desc(),
        request_handler: Some(&mut request_handler),
        poll_ms: 60,
        max_packet_size: 8,
        hid_subclass: HidSubclass::Boot,
        hid_boot_protocol: HidBootProtocol::Keyboard,
    };

    let mut writer = HidWriter::<_, 8>::new(&mut builder, &mut hid_state, hid_config);

    // Build the builder.
    let mut usb = builder.build();

    // Run the USB device.
    let usb_fut = usb.run();

    // Do stuff with the CDC ACM class!
    let echo_fut = async {
        loop {
            class.wait_connection().await;
            info!("CDC Connected");
            let _ = echo(&mut class).await;
            info!("CDC Disconnected");
        }
    };

    let mut s2 = Input::new(p.PA7, Pull::Up);

    // Do stuff with the HID class!
    let out_fut = async {
        loop {
            info!("Waiting for button press (falling edge)");
            s2.wait_for_falling_edge().await;
            info!("Button pressed!");

            if HID_PROTOCOL_MODE.load(Ordering::Relaxed) == HidProtocolMode::Boot as u8 {
                match writer.write(&[0, 0, 4, 0, 0, 0, 0, 0]).await {
                    Ok(()) => {}
                    Err(e) => warn!("Failed to send boot report: {:?}", e),
                }
            } else {
                let report = KeyboardReport {
                    keycodes: [4, 0, 0, 0, 0, 0], // 'A' or 'a' key
                    leds: 0,
                    modifier: 0,
                    reserved: 0,
                };
                match writer.write_serialize(&report).await {
                    Ok(()) => {}
                    Err(e) => warn!("Failed to send report: {:?}", e),
                }
            }

            info!("Waiting for button release (rising edge)");
            s2.wait_for_rising_edge().await;
            info!("Button released!");

            if HID_PROTOCOL_MODE.load(Ordering::Relaxed) == HidProtocolMode::Boot as u8 {
                match writer.write(&[0, 0, 0, 0, 0, 0, 0, 0]).await {
                    Ok(()) => {}
                    Err(e) => warn!("Failed to send boot report: {:?}", e),
                }
            } else {
                let report = KeyboardReport {
                    keycodes: [0, 0, 0, 0, 0, 0],
                    leds: 0,
                    modifier: 0,
                    reserved: 0,
                };
                match writer.write_serialize(&report).await {
                    Ok(()) => {}
                    Err(e) => warn!("Failed to send report: {:?}", e),
                }
            }
        }
    };

    // Run everything concurrently.
    join3(usb_fut, echo_fut, out_fut).await;
}

struct Disconnected {}

impl From<EndpointError> for Disconnected {
    fn from(val: EndpointError) -> Self {
        match val {
            EndpointError::BufferOverflow => panic!("Buffer overflow"),
            EndpointError::Disabled => Disconnected {},
        }
    }
}

async fn echo<'d>(
    class: &mut CdcAcmClass<'d, embassy_mspm0::usb::Driver<'d, embassy_mspm0::peripherals::USBFS0>>,
) -> Result<(), Disconnected> {
    let mut buf = [0; 64];
    loop {
        let n = class.read_packet(&mut buf).await?;
        let data = &buf[..n];
        info!("data: {:x}", data);
        class.write_packet(data).await?;
    }
}

struct MyRequestHandler {}

impl RequestHandler for MyRequestHandler {
    fn get_report(&mut self, id: ReportId, _buf: &mut [u8]) -> Option<usize> {
        info!("Get report for {:?}", id);
        None
    }

    fn set_report(&mut self, id: ReportId, data: &[u8]) -> OutResponse {
        info!("Set report for {:?}: {=[u8]}", id, data);
        OutResponse::Accepted
    }

    fn get_protocol(&self) -> HidProtocolMode {
        let protocol = HidProtocolMode::from(HID_PROTOCOL_MODE.load(Ordering::Relaxed));
        info!("The current HID protocol mode is: {}", protocol);
        protocol
    }

    fn set_protocol(&mut self, protocol: HidProtocolMode) -> OutResponse {
        info!("Switching to HID protocol mode: {}", protocol);
        HID_PROTOCOL_MODE.store(protocol as u8, Ordering::Relaxed);
        OutResponse::Accepted
    }

    fn set_idle_ms(&mut self, id: Option<ReportId>, dur: u32) {
        info!("Set idle rate for {:?} to {:?}", id, dur);
    }

    fn get_idle_ms(&mut self, id: Option<ReportId>) -> Option<u32> {
        info!("Get idle rate for {:?}", id);
        None
    }
}
