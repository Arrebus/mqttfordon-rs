#![allow(dead_code)]
#![allow(unused_variables)]
#![allow(unused_imports)]

use crate::controllerhal::PCA9634;
use crate::leddriver::WS2812RMT;
use anyhow::Result;
use embedded_hal::digital::OutputPin;
use embedded_svc::mqtt::client;
use esp_idf_hal::{gpio::PinDriver, i2c, peripherals::Peripherals, units::*};
use esp_idf_svc::{
    //wifi::EspWifi,
    eventloop::EspSystemEventLoop,
    hal::i2c::{I2cConfig, I2cDriver},
    mqtt::client::MqttProtocolVersion,
    nvs::EspDefaultNvsPartition,
};
use esp_idf_sys as _;
use log::debug;
use shared_bus;
use std::{
    env,
    sync::{Arc, Mutex},
    thread::{self, sleep},
    time::Duration, //for threads!
};
//use controllerhal::{DeviceAddr, PCA9634};
mod controllerhal;
mod leddriver;
mod mqtt;
mod wifi;
//mod ctrl;

fn main() {
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    let peripherals = Peripherals::take().unwrap();
    let sys_loop = EspSystemEventLoop::take().unwrap();
    let nvs = EspDefaultNvsPartition::take().unwrap();

    //get environmental variables so that other coding devices can have different configurations.
    const WIFI_SSID: &str = env!("WIFI_SSID");
    const WIFI_PASSWORD: &str = env!("WIFI_PASSWORD");
    const MQTT_ADRESS: &str = env!("MQTT_ADRESS");
    const FORDON_ID: &str = env!("FORDON_ID");

    //----------------------I2C och Styrsystem setup----------------------
    //let mut oe = PinDriver::output(peripherals.pins.gpio1).unwrap();

    let mut oe = PinDriver::output(peripherals.pins.gpio1).unwrap();
    let led = peripherals.pins.gpio2;
    let channel = peripherals.rmt.channel0;
    let mut ws2812 = WS2812RMT::new(led, channel).unwrap();
    let _ = oe.set_high();
    sleep(Duration::from_millis(4000));

    sleep(Duration::from_millis(10));
    let sda = peripherals.pins.gpio10;
    let scl = peripherals.pins.gpio8;

    let config = I2cConfig::new().baudrate(100.kHz().into());
    let i2c: I2cDriver<'static> = I2cDriver::new(peripherals.i2c0, sda, scl, &config).unwrap();

    //shared bus configuration
    //let bus: &'static _ = shared_bus::new_std!(I2cDriver<'_> = i2c).unwrap();
    debug!("-----STARTAR STYRSYSTEM-----");
    let mut styrsystem: PCA9634<I2cDriver<'static>> =
        controllerhal::PCA9634::new(i2c, controllerhal::DeviceAddr::DEFADR);
    debug!("Initierar register...");
    styrsystem.init_controller();
    let _ = oe.set_low();
    debug!("Provkör!");
    //styrsystem.drive();
    debug!("----------------------------");
    let styrsystem = Arc::new(Mutex::new(styrsystem));
    //let styrsystem = Arc::new(styrsystem);
    //let styrsystem_clone = Arc::clone(&styrsystem)
    //--------------------------------------------------------------------

    //-----------------------------WIFI-modul-----------------------------
    //Creates and returns an wifi driver
    let wifi_driver = wifi::anslut(&sys_loop, &nvs, peripherals.modem, WIFI_SSID, WIFI_PASSWORD);
    //--------------------------------------------------------------------

    //----------------------------MQTT Klient-----------------------------
    //Creating Atomic Reference Counting for handling of controller instance in concurrency
    let styrsys_mqtt_clone = Arc::clone(&styrsystem);
    let client = mqtt::mqtt_init(MQTT_ADRESS, styrsys_mqtt_clone, FORDON_ID);

    let client = Arc::new(Mutex::new(client));
    //--------------------------------------------------------------------
    //Subscribe to topic in a temporary scope. Creaates a clone reference that dies at the end of scope.
    {
        let client_clone = Arc::clone(&client);
        let mut c = client_clone.lock().unwrap();
        c.subscribe("/user/#", client::QoS::AtLeastOnce).unwrap();
    }

    let client_clone = Arc::clone(&client);
    let styrsys_clone = Arc::clone(&styrsystem);

    //Thread
    //TODO: Move thread contents to own function: thread::spawn(move|| speedThread());
    thread::spawn(move || {
        loop {
            {
                // !!!IMPORTANT!!! scope inside the loop so that clientmutex can be unlocked between the different threads!
                let mut client = client_clone.lock().unwrap();
                let styrsystem = styrsystem.lock().unwrap();
                //let data = format!("{{\"carID\":\"{}\", \"speed\": {}}}", FORDON_ID, styrsystem.get_speed());
                let data = format!("{{\"carID\":\"{}\", \"speed\": {}}}", FORDON_ID, 5);
                // debug!("Sending data:\n{}", data);
                //Turned of retain so that the broker doesnt save messages between runs and
                //sessions!
                client
                    .publish(
                        "/vehicle/publishSpeed",
                        client::QoS::AtLeastOnce,
                        false,
                        data.as_bytes(),
                    )
                    .expect("Kunde ej publisera");
            }
            sleep(Duration::from_secs(5));
        }
    });

    let emergency_clone = Arc::clone(&styrsys_clone);
    thread::spawn(move || {
        loop {
            {
                let mut e = emergency_clone.lock().unwrap();
                match e.get_emergency_stop() {
                    true => {
                        //debug!("true");
                        ws2812.set_pixel(rgb::RGB8::new(255, 0, 0)).unwrap();
                        sleep(Duration::from_millis(100));
                        ws2812.set_pixel(rgb::RGB8::new(0, 0, 255)).unwrap();
                    }
                    false => {
                        ws2812.set_pixel(rgb::RGB8::new(0, 0, 0)).unwrap();
                        //debug!("False")
                    }
                }
            }
            sleep(Duration::from_millis(100));
        }
    });

    loop {
        thread::sleep(Duration::from_secs(3600)); //So that main does not return
    }
}
