use crate::controllerhal::PCA9634;
use embedded_svc::mqtt::client::QoS;
use embedded_svc::{
    io::ErrorKind,
    mqtt::{
        self,
        client::{Event, Message},
    },
};
use esp_idf_svc::{
    hal::i2c::{I2cConfig, I2cDriver},
    mqtt::client::{EspMqttClient, EspMqttMessage, MqttClientConfiguration},
};
use json::{self, JsonValue};
use log::debug;
use serde_json::{from_slice, from_str, Value};
use std::{
    os::unix::net::UnixDatagram,
    str::FromStr,
    sync::{Arc, Mutex},
    thread::{self, sleep},
    time::Duration,
};

pub fn mqtt_init(
    mqttadr: &str,
    styrsystem: Arc<Mutex<PCA9634<I2cDriver<'static>>>>,
    carid: &str,
) -> EspMqttClient<'static> {
    esp_idf_sys::link_patches();

    let mqtt_config = MqttClientConfiguration::default();
    let carid = carid.to_owned();
    // Creates client and definition of event
    let client = EspMqttClient::new(mqttadr, &mqtt_config, move |message_event| {
        let styrsystem = Arc::clone(&styrsystem);
        match message_event.as_ref().unwrap() {
            Event::Connected(_) => debug!("Connected"),
            Event::Subscribed(id) => debug!("Subscribed to {} id", id),
            Event::Received(msg) => handle_message(msg, styrsystem, &carid),
            Event::Published(msg) => (),
            _ => debug!("{:?}", message_event.as_ref().unwrap()),
        };
    })
    .unwrap();
    client
}
///Private function that handles messages received by vehicle
/// TODO: Add handling depending on topic and ID. Preferrably in different functions or such.
fn handle_message(
    msg: &EspMqttMessage,
    styrsystem: Arc<Mutex<PCA9634<I2cDriver<'_>>>>,
    carid: &str,
) {
    match msg.topic() {
        Some("/user/setSpeed") => set_vehicle_speed(msg.data(), styrsystem, &carid),
        Some("/user/maxSpeed") => set_max_speed(msg.data(), styrsystem, &carid),
        Some("/user/emergencyStop") => emergency_stop_id(msg.data(), styrsystem, &carid),
        Some("/user/emergencyStopAll") => emergency_stop(msg.data(), styrsystem),
        //keyboard commands
        Some("/user/keyboard") => keyboard(msg.data(), styrsystem, carid),
        Some("/user/blockbuilder") => instructions(msg.data(), styrsystem, carid),
        _ => {}
    }
}
//Gets value (boolean) from mqtt-emergency stop.
//Sende value to controller and puts actual value to the vehicle.
fn emergency_stop(data: &[u8], styrsystem: Arc<Mutex<PCA9634<I2cDriver<'_>>>>) {
    match convert_to_json(data) {
        Ok(jsondata) => {
            let car_state = jsondata.as_bool().unwrap();
            {
                let mut styrsystem = styrsystem.lock().unwrap();
                styrsystem.set_emergency_stop(car_state);
            }
        }
        Err(e) => {
            debug!("{}", e);
        }
    }
}

fn emergency_stop_id(data: &[u8], styrsystem: Arc<Mutex<PCA9634<I2cDriver<'_>>>>, carid: &str) {
    match convert_to_json(data) {
        Ok(jsondata) => {
            if let Some(id) = jsondata["carID"].as_str() {
                if id == carid {
                    if let Some(emstop) = jsondata["state"].as_bool() {
                        let mut styrsystem = styrsystem.lock().unwrap();
                        styrsystem.set_emergency_stop(emstop);
                    } else {
                        debug!("kunde ej konvertera speed till sträng");
                    }
                }
            } else {
                debug!("ID matchar ej.");
            }
        }
        Err(e) => {
            debug!("{}", e);
        }
    };
}
///Sets the speed of the vehicle
fn set_vehicle_speed(data: &[u8], styrsystem: Arc<Mutex<PCA9634<I2cDriver<'_>>>>, carid: &str) {
    match convert_to_json(data) {
        Ok(jsondata) => {
            if let Some(id) = jsondata["carID"].as_str() {
                //Check so that ID is parsed
                if id == carid {
                    //Vehicle in question
                    if let Some(speed_str) = jsondata["speed"].as_str() {
                        match <i32 as FromStr>::from_str(speed_str) {
                            Ok(speed) => {
                                if speed <= 100 && speed >= -100 {
                                    {
                                        let mut styrsystem = styrsystem.lock().unwrap();
                                        styrsystem.set_speed(speed);
                                        styrsystem.read_all_addresses();
                                    }

                                    debug!("Tog emot meddelande!");
                                } else {
                                    debug!("Hastighet är utanför tillåten räckvid (-100 - 100)!");
                                }
                            }
                            Err(_) => {
                                debug!("Kunde ej parsera hastighet!");
                            }
                        }
                    } else {
                        debug!("kunde ej konvertera speed till sträng");
                    }
                }
            } else {
                debug!("ID matchar ej.");
            }
        }
        Err(e) => {
            debug!("{}", e);
        }
    };
}
fn set_max_speed(data: &[u8], styrsystem: Arc<Mutex<PCA9634<I2cDriver<'_>>>>, carid: &str) {
    match convert_to_json(data) {
        Ok(jsondata) => {
            if let Some(id) = jsondata["carID"].as_str() {
                if id == carid {
                    if let Some(max_str) = jsondata["max"].as_str() {
                        match <i32 as FromStr>::from_str(max_str) {
                            Ok(maxspeed) => {
                                if maxspeed <= 100 && maxspeed >= -100 {
                                    let mut styrsystem = styrsystem.lock().unwrap();
                                    styrsystem.set_max_speed(maxspeed);
                                }
                            }
                            Err(_) => {
                                debug!("Kunde ej parsera hastighet!");
                            }
                        }
                    } else {
                        debug!("kunde ej konvertera maxhastighet till sträng");
                    }
                }
            } else {
                debug!("ID matchar ej.");
            }
        }
        Err(e) => {
            debug!("{}", e);
        }
    };
}

fn parse_json_to_i32(data: Option<&str>) -> i32 {
    match data {
        Some(data) => {
            let speed: i32 = data.parse().unwrap_or(0);
            speed
        }
        None => {
            debug!("Kunde ej konvertera hastighet!");
            0
        }
    }
}

///Konverterar Bytedata till JSON.
fn convert_to_json(data: &[u8]) -> Result<JsonValue, &str> {
    //konvertera bytes till sträng
    match std::str::from_utf8(data) {
        Ok(strdata) => {
            //Uncomment when debugging
            /*debug!("*****SET SPEED STRÄNG*****");
            debug!("{}", strdata);
            debug!("**************************");*/

            //convert to jsondata!
            match json::parse(strdata) {
                Ok(jsondata) => {
                    //Avkommentera vid debugging
                    /*debug!("---JSON DATA---");
                    debug!("{}", jsondata);
                    debug!("---------------");*/
                    Ok(jsondata)
                }
                Err(e) => Err("Kunde ej konvertera sträng till JSON"),
            }
        }
        Err(e) => Err("Kunde ej konvertera Data till sträng"),
    }
}

//Keyboard controll
fn keyboard(data: &[u8], styrsystem: Arc<Mutex<PCA9634<I2cDriver<'_>>>>, carid: &str) {
    debug!("keyboard command");
    match convert_to_json(data) {
        Ok(jsondata) => {
            if let Some(id) = jsondata["carID"].as_str() {
                if id == carid {
                    if let Some(state) = jsondata["state"].as_bool() {
                        if let Some(speed) = jsondata["speed"].as_i32() {
                            if let Some(direction) = jsondata["direction"].as_i32() {
                                debug!("keyboard: {state}, {speed}, {direction}");
                                let mut styrsystem = styrsystem.lock().unwrap();
                                styrsystem.keyboard_control(direction, state, speed);
                            } else {
                                debug!("direction error");
                            }
                        } else {
                            debug!("speed error");
                        }
                    } else {
                        debug!("state error");
                    }
                }
            } else {
                debug!("id error");
            }
        }
        Err(e) => {
            debug!("{}", e);
        }
    };
}

fn instructions(data: &[u8], styrsystem: Arc<Mutex<PCA9634<I2cDriver<'_>>>>, carid: &str) {
    debug!("keyboard command");
    println!("------ Instruktion kommando -----");
    let jsondata: Value = from_slice(data).expect("Kunde ej parsera JSON");
    if let Some(id) = jsondata["id"].as_str() {
    println!("mottaget id: {id} carid: {carid}");
        if id == carid {
            if let Some(instructions) = jsondata["instructions"].as_array() {
                for instruction in instructions {
                    println!("Instruktion!");                    
                    if let Some(meters) = instruction["forward"].as_i64() {
                        println!("Forward start!");
                        let mut styrsystem = styrsystem.lock().unwrap();
                        styrsystem.inst_forward(meters as i32);
                    } else if let Some(meters) = instruction["backward"].as_i64() {
                         println!("Backward start!");
                         let mut styrsystem = styrsystem.lock().unwrap();
                        styrsystem.inst_backward(meters as i32);
                    } else if let Some(rotatel) = instruction["rotateL"].as_i64() {
                        println!("Rotate Left start!");
                        let mut styrsystem = styrsystem.lock().unwrap();
                        styrsystem.inst_rotate_l(rotatel as i32);
                    } else if let Some(rotater) = instruction["rotateR"].as_i64() {
                        println!("Rotate Right start!");
                        let mut styrsystem = styrsystem.lock().unwrap();
                        styrsystem.inst_rotate_r(rotater as i32);
                    }
                }
            } else{ println!("fel på as array"); }
        }
    } else{println!("fel på id");}
}

