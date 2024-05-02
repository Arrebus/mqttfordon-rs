use embedded_svc::wifi::{AuthMethod, ClientConfiguration, Configuration};
use esp_idf_hal::modem::Modem;
use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use esp_idf_svc::wifi::BlockingWifi;
use esp_idf_svc::wifi::EspWifi;
use esp_idf_sys as _;
use log::debug;

///Connects wifi-card to wifi and returns a wifi instance.
pub fn anslut(
    sys_loop: &EspSystemEventLoop,
    nvs: &EspDefaultNvsPartition,
    modem: Modem,
    adr: &str,
    pwd: &str,
) -> BlockingWifi<EspWifi<'static>> {
    debug!("Startar Wifi!");
    esp_idf_sys::link_patches();

    let mut wifi_driver = BlockingWifi::wrap(
        EspWifi::new(modem, sys_loop.clone(), Some(nvs.clone())).unwrap(),
        sys_loop.clone(),
    )
    .unwrap();

    wifi_driver
        .set_configuration(&Configuration::Client(ClientConfiguration {
            ssid: adr.into(),
            bssid: None,
            auth_method: AuthMethod::WPA2Personal,
            password: pwd.into(),
            channel: None,
        }))
        .unwrap();

    wifi_driver.start().unwrap();
    wifi_driver.connect().unwrap();
    wifi_driver.wait_netif_up().unwrap();
    // Print Out Wifi Connection Configuration
    while !wifi_driver.is_connected().unwrap() {
        //print network configuration
        let config = wifi_driver.get_configuration().unwrap();
        debug!("Waiting for station {:?}", config);
    }
    debug!("Ansluten!");
    wifi_driver
}
