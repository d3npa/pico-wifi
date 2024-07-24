#![no_std]

use cyw43::{Control, NetDriver};
use cyw43_pio::PioSpi;
use embassy_executor::Spawner;
use embassy_net::{ConfigV6, Stack, StackResources};
use embassy_net::{
    Ipv4Address, Ipv4Cidr, Ipv6Address, Ipv6Cidr, StaticConfigV4,
};
use embassy_rp::bind_interrupts;
use embassy_rp::gpio::Output;
use embassy_rp::peripherals::{DMA_CH0, PIN_23, PIN_25, PIO0};
use embassy_rp::pio::InterruptHandler;
use embassy_time::Timer;
use heapless::Vec;
use static_cell::StaticCell;

type Pwr = Output<'static, PIN_23>;
type Spi = PioSpi<'static, PIN_25, PIO0, 0, DMA_CH0>;

pub static STATE: StaticCell<cyw43::State> = StaticCell::new();
pub static STACK: StaticCell<Stack<NetDriver<'static>>> = StaticCell::new();
pub static RESOURCES: StaticCell<StackResources<2>> = StaticCell::new();

bind_interrupts!(pub struct Irqs {
    PIO0_IRQ_0 => InterruptHandler<PIO0>;
});

#[embassy_executor::task]
async fn wifi_task(runner: cyw43::Runner<'static, Pwr, Spi>) -> ! {
    runner.run().await
}

#[embassy_executor::task]
async fn net_task(stack: &'static Stack<NetDriver<'static>>) -> ! {
    stack.run().await
}

pub async fn configure_network(
    spawner: &Spawner,
    pwr: Pwr,
    spi: Spi,
    ssid: &str,
    password: Option<&str>,
) -> (Control<'static>, &'static Stack<NetDriver<'static>>) {
    let (net_dev, ctrl) = init_wifi(&spawner, pwr, spi, ssid, password).await;
    let stack = init_ip(&spawner, net_dev).await;

    while !stack.is_config_up() {
        Timer::after_millis(100).await;
    }

    return (ctrl, stack);
}

async fn init_wifi(
    spawner: &Spawner,
    pwr: Pwr,
    spi: Spi,
    ssid: &str,
    password: Option<&str>,
) -> (NetDriver<'static>, Control<'static>) {
    /* init wifi */

    let fw = include_bytes!("../cyw43-firmware/43439A0.bin");
    let clm = include_bytes!("../cyw43-firmware/43439A0_clm.bin");

    let state = STATE.init(cyw43::State::new());
    let (dev, mut ctrl, runner) = cyw43::new(state, pwr, spi, fw).await;

    let _ = spawner.spawn(wifi_task(runner));

    ctrl.init(clm).await;
    ctrl.set_power_management(cyw43::PowerManagementMode::PowerSave)
        .await;

    if let Some(password) = password {
        ctrl.gpio_set(0, true).await;
        Timer::after_secs(1).await;
        ctrl.gpio_set(0, false).await;

        if ctrl.join_wpa2(ssid, password).await.is_err() {
            /* require reboot when failed to join wifi. network code won't run
            but other tasks, which were already spawned, will continue */
            todo!(
            "exit async task without disrupting others. maybe using Result?"
        );
        }

        ctrl.gpio_set(0, true).await;
        Timer::after_secs(1).await;
        ctrl.gpio_set(0, false).await;
    } else {
        todo!("handle connecting to open networks");
    }

    (dev, ctrl)
}

async fn init_ip(
    spawner: &Spawner,
    net_dev: NetDriver<'static>,
) -> &'static Stack<NetDriver<'static>> {
    /* init ip stack */

    let ipv4_config = StaticConfigV4 {
        address: Ipv4Cidr::new(Ipv4Address::new(192, 168, 1, 6), 24),
        dns_servers: Vec::new(),
        gateway: Some(Ipv4Address::new(192, 168, 1, 1)),
    };

    let mut dns_servers = Vec::<Ipv6Address, 3>::new();
    dns_servers
        .push(Ipv6Address::new(
            0xfd00, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0001,
        ))
        .unwrap();

    let ipv6_config = embassy_net::StaticConfigV6 {
        address: Ipv6Cidr::new(
            Ipv6Address::new(0xfd00, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0006),
            64,
        ),
        gateway: Some(Ipv6Address::new(
            0xfd00, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0001,
        )),
        dns_servers,
    };

    let mut config = embassy_net::Config::ipv4_static(ipv4_config);
    config.ipv6 = ConfigV6::Static(ipv6_config);

    let seed = 0x0123_4567_89ab_cdef;

    // Init network stack

    let stack = STACK.init(Stack::new(
        net_dev,
        config,
        RESOURCES.init(StackResources::<2>::new()),
        seed,
    ));

    let _ = spawner.spawn(net_task(stack));
    stack
}
