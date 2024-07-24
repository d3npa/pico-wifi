#![no_std]

use cyw43::{Control, NetDriver};
use cyw43_pio::PioSpi;
use embassy_executor::Spawner;
use embassy_net::StaticConfigV4;
use embassy_net::{Stack, StackResources, StaticConfigV6};
use embassy_rp::bind_interrupts;
use embassy_rp::gpio::Output;
use embassy_rp::peripherals::{DMA_CH0, PIN_23, PIN_25, PIO0};
use embassy_rp::pio::InterruptHandler;
use embassy_time::Timer;
use static_cell::StaticCell;

type Pwr = Output<'static, PIN_23>;
type Spi = PioSpi<'static, PIN_25, PIO0, 0, DMA_CH0>;

pub static STATE: StaticCell<cyw43::State> = StaticCell::new();
pub static STACK: StaticCell<Stack<NetDriver<'static>>> = StaticCell::new();
pub static RESOURCES: StaticCell<StackResources<2>> = StaticCell::new();

bind_interrupts!(pub struct Irqs {
    PIO0_IRQ_0 => InterruptHandler<PIO0>;
});

#[derive(Clone, Default)]
pub struct WifiConfiguration<'a> {
    pub wifi_ssid: &'a str,
    pub wifi_password: Option<&'a str>,
    pub ipv4: Option<StaticConfigV4>,
    pub ipv6: Option<StaticConfigV6>,
}

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
    config: WifiConfiguration<'_>,
) -> (Control<'static>, &'static Stack<NetDriver<'static>>) {
    let (net_dev, ctrl) =
        init_wifi(&spawner, pwr, spi, config.wifi_ssid, config.wifi_password)
            .await;
    let stack = init_ip(&spawner, net_dev, config.ipv4, config.ipv6).await;

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
    ipv4_config: Option<StaticConfigV4>,
    ipv6_config: Option<StaticConfigV6>,
) -> &'static Stack<NetDriver<'static>> {
    /* init ip stack */

    let config = {
        if let Some(ipv4_config) = ipv4_config {
            embassy_net::Config::ipv4_static(ipv4_config)
        } else if let Some(ipv6_config) = ipv6_config {
            embassy_net::Config::ipv6_static(ipv6_config)
        } else {
            todo!("handle cases when neither ipv4 nor ipv6 are provided");
        }
    };

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
