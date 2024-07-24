# pico-wifi

ラズパイpico wで新しいプロジェクトをはじめるたびに、wifiへ接続させるコードをほぼ丸ごとコピペしていたので、クレートにしました。

## 用例

```rs
use heapless::Vec;
use embassy_net::{Ipv4Address, Ipv4Cidr, StaticConfigV4};
use pico_wifi::{configure_network, WifiConfiguration};

let wifi_config = WifiConfiguration {
    wifi_ssid: WIFI_SSID,
    wifi_password: Some(WIFI_PASSWORD),
    ipv4: Some(StaticConfigV4 {
        address: Ipv4Cidr::new(Ipv4Address::new(192, 168, 1, 6), 24),
        gateway: Some(Ipv4Address::new(192, 168, 0, 1)),
        dns_servers: Vec::new(),
    }),
    ipv6: None,
};

let (ctrl, stack) =
    configure_network(&spawner, pwr, spi, wifi_config).await;
```
