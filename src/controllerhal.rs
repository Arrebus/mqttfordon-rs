use embedded_hal::i2c::{self, Error, I2c};
use esp_idf_sys::EspError;
use log::debug;
use std::{thread::sleep, time::Duration};

#[derive(Copy, Clone)]
pub struct PCA9634<I2C> {
    i2c: I2C,
    //Communikationsadresser
    address: DeviceAddr,

    speed: i32,
    maxspeed: i32,
    emergency_stop: bool,
}

impl<I2C: I2c> PCA9634<I2C> {
    pub fn new(i2c: I2C, address: DeviceAddr) -> Self {
        Self {
            i2c,
            address,
            speed: 0,
            maxspeed: 100,
            emergency_stop: false,
        }
    }

    ///Initialize controller with the vaules needed for SCB Motordrive3
    pub fn init_controller(&mut self) {
        //self.software_reset();
        sleep(Duration::from_millis(6));
        let mode1 = 0x01;
        let mode2: u8 = 0x14;
        //Write startvariables to the vehicle
        self.write_register(Register::MODE2, mode2);
        self.write_register(Register::MODE1, mode1);
        sleep(Duration::from_millis(6)); // vänta på socillator
                                         //Tillåt PWM styrning
        self.write_register(Register::LEDOUT0, 0xFF);
        self.write_register(Register::LEDOUT1, 0xFF);
    }
    ///For debug... Reads all addresses on the PCA9634
    pub fn read_all_addresses(&mut self) {
        self.read_register(Register::MODE1);
        self.read_register(Register::MODE2);
        self.read_register(Register::PWM0);
        self.read_register(Register::PWM1);
        self.read_register(Register::PWM2);
        self.read_register(Register::PWM3);
        self.read_register(Register::PWM4);
        self.read_register(Register::PWM5);
        self.read_register(Register::PWM6);
        self.read_register(Register::PWM7);
        self.read_register(Register::GRPPWM);
        self.read_register(Register::GRPFREQ);
        self.read_register(Register::LEDOUT0);
        self.read_register(Register::LEDOUT1);
        self.read_register(Register::SUBADR1);
        self.read_register(Register::SUBADR2);
        self.read_register(Register::SUBADR3);
        self.read_register(Register::ALLCALLADR);
    }
    ///Writes values to the PCA9634 via i2c interface
    fn write_register(&mut self, register: Register, value: u8) {
        let byte = value;
        self.i2c
            .write(self.address as u8, &[register.address(), byte])
            .unwrap();
    }

    ///Reads values to PCA9634 via i2c interface
    fn read_register(&mut self, register: Register) -> u8 {
        let mut data = [0];
        self.i2c
            .write_read(self.address as u8, &[register.address()], &mut data)
            .unwrap();
        debug!("Reg: {}: {:02x}", register.to_string(), data[0]);
        u8::from_le_bytes(data)
    }

    ///Software reset According to data sheet for PCA9634. Har ej använts i något syfte men bra att ha kvar.
    pub fn software_reset(&mut self) {
        let software_reset_address: u8 = 0x03;
        let reset_sequence: [u8; 2] = [0xA5, 0x5A];
        self.i2c
            .write(DeviceAddr::SFTRESET.address() as u8, &reset_sequence)
            .unwrap();
    }

    // --------------- Getters & Setters for vehicle---------------
    ///Sets emergency stop for the controller
    pub fn set_emergency_stop(&mut self, car_state: bool) {
        self.emergency_stop = car_state;
        debug!("Emergency stop = {}", self.emergency_stop);
        if self.emergency_stop {
            self.stop_vehicle();
        }
    }

    pub fn get_emergency_stop(&mut self) -> bool {
        self.emergency_stop
    }
    ///Sets max speed. If current speed is greater or less than (forwards or backwards) a new allowed speed will be set.
    pub fn set_max_speed(&mut self, max: i32) {
        //debug!("Sätter maxhastighet till {max}");
        if self.speed > max {
            self.forward(self.calculate_speed(max) as u8);
        }
        if self.speed < (max * -1) {
            self.backwards(self.calculate_speed(max) as u8);
        }
        self.maxspeed = max;
    }

    pub fn get_max_speed(&mut self) -> i32 {
        self.maxspeed
    }

    ///Calculates the speed that is applied to the motors. Current hardware has a minimum of 190 and max 255, which means we need to translate 0-100 -> 190-255
    fn calculate_speed(&self, speed: i32) -> i32 {
        let calculated_speed = (speed * 65) / 100 + 190;
        debug!("Calculated speed: {}", calculated_speed);
        calculated_speed
    }

    ///Calculates the speed that is applied to the motors backwards. Current hardware has a minimum of 190 and max 255, which means we need to translate 0-100 -> 190-255
    fn calculate_bwd(&self, speed: i32) -> i32 {
        let calculated_speed = (i32::abs(speed) * 65) / 100 + 190;
        debug!("Calculated speed: {}", calculated_speed);
        calculated_speed
    }

    /// Applies speed to the vehice.
    pub fn set_speed(&mut self, mut speed: i32) {
        if !self.emergency_stop {
            match speed {
                1..=100 => {
                    if speed > self.maxspeed {
                        speed = self.maxspeed
                    }
                    self.speed = speed;
                    self.forward(self.calculate_speed(speed) as u8);
                }
                -100..=-1 => {
                    if speed < (self.maxspeed * -1) {
                        speed = self.maxspeed * -1;
                    }
                    self.speed = speed;
                    self.backwards(self.calculate_bwd(speed) as u8);
                }
                0 => self.stop_vehicle(),
                _ => {}
            }
        }
    }

    //------Driving functions-------
    /// Drives forward. Forward channel in PWM is PWM 0,2,4,6 Also clears backwards driving so that they dont interfere with one another
    fn forward(&mut self, speed: u8) {
        self.clear_backwards();
        self.write_register(Register::PWM0, speed);
        self.write_register(Register::PWM2, speed);
        self.write_register(Register::PWM4, speed);
        self.write_register(Register::PWM6, speed);
    }

    /// Clears forward registers
    fn clear_forward(&mut self) {
        self.write_register(Register::PWM0, 0);
        self.write_register(Register::PWM2, 0);
        self.write_register(Register::PWM4, 0);
        self.write_register(Register::PWM6, 0);
        sleep(Duration::from_millis(2));
    }

    /// Drives backward.Forward channel in PWM is PWM 0,2,4,6 Also clears backwards driving so that they dont interfere with one another
    fn backwards(&mut self, speed: u8) {
        self.clear_forward();
        self.write_register(Register::PWM1, speed);
        self.write_register(Register::PWM3, speed);
        self.write_register(Register::PWM5, speed);
        self.write_register(Register::PWM7, speed);
    }

    /// Clears backward registers
    fn clear_backwards(&mut self) {
        self.write_register(Register::PWM1, 0);
        self.write_register(Register::PWM3, 0);
        self.write_register(Register::PWM5, 0);
        self.write_register(Register::PWM7, 0);
        sleep(Duration::from_millis(2));
    }

    //Fetch speed
    pub fn get_speed(&mut self) -> i32 {
        self.speed
    }
    ///Stops Vehicle completely.
    pub fn stop_vehicle(&mut self) {
        self.speed = 0;
        self.write_register(Register::PWM1, 0);
        self.write_register(Register::PWM3, 0);
        self.write_register(Register::PWM5, 0);
        self.write_register(Register::PWM7, 0);

        self.write_register(Register::PWM0, 0);
        self.write_register(Register::PWM2, 0);
        self.write_register(Register::PWM4, 0);
        self.write_register(Register::PWM6, 0);
        debug!("Fordonet stoppat!")
    }

    //---------------------- Keyboardstyrning ------------------------
    /*
       Hur direction fungerar:
        1
      8 ^ 2
      \ |/
    7<- . -> 3
      / |\
     6  v 4
        5
       */
    pub fn keyboard_control(&mut self, direction: i32, state: bool, speed: i32) {
        if !self.emergency_stop {
            if !state {
                self.stop_vehicle();
            } else {
                let speed = self.calculate_speed(speed) as u8;
                match direction {
                    1 => self.forward(speed),
                    2 => self.turn(speed / 2, speed, true, true),
                    3 => self.turn(0, speed, true, true),
                    4 => self.turn(speed / 2, speed, true, false),
                    5 => self.backwards(speed),
                    6 => self.turn(speed / 2, speed, false, false),
                    7 => self.turn(0, speed, false, true),
                    8 => self.turn(speed / 2, speed, false, true),
                    _ => self.stop_vehicle(),
                }
            }
        }
        debug!("Exiting keyboard control...");
    }
    //(front left wheel)
    fn fl_wheel(&mut self, forward: u8, backward: u8) {
        self.write_register(Register::PWM4, forward);
        self.write_register(Register::PWM5, backward);
    }

    //(front right wheel)
    fn fr_wheel(&mut self, forward: u8, backward: u8) {
        self.write_register(Register::PWM6, forward);
        self.write_register(Register::PWM7, backward);
    }

    //(back right wheel)
    fn br_wheel(&mut self, forward: u8, backward: u8) {
        self.write_register(Register::PWM0, forward);
        self.write_register(Register::PWM1, backward);
    }

    fn bl_wheel(&mut self, forward: u8, backward: u8) {
        self.write_register(Register::PWM2, forward);
        self.write_register(Register::PWM3, backward);
    }

    fn turn(&mut self, iw: u8, ow: u8, right: bool, fwd: bool) {
        if fwd {
            if right {
                //Sväng: höger framåt
                self.bl_wheel(ow, 0);
                self.fl_wheel(ow, 0);
                self.br_wheel(iw, 0);
                self.fr_wheel(iw, 0);
            } else {
                //Sväng: vänster framåt
                self.bl_wheel(iw, 0);
                self.fl_wheel(iw, 0);
                self.br_wheel(ow, 0);
                self.fr_wheel(ow, 0);
            }
        } else {
            if right {
                //Sväng: höger bakåt... Vänstra hjulen bakåt
                self.bl_wheel(0, ow);
                self.fl_wheel(0, ow);
                self.br_wheel(0, iw);
                self.fr_wheel(0, iw);
            } else {
                //Sväng: vänster bakåt... Högra hjulen bakåt
                self.bl_wheel(0, iw);
                self.fl_wheel(0, iw);
                self.br_wheel(0, ow);
                self.fr_wheel(0, ow);
            }
        }
    }
    //---------------------------------------------
    //--------------- INSTRUCTIONS ---------------
    ///Rotate vehicle X amount degrees left
    pub fn inst_rotate_l(&mut self, degrees: i32) {
        println!("Rotating {degrees} degrees left!");
        self.rotation(75, true);
        sleep(Duration::from_millis(self.calculate_degree_sleep(degrees))); // This will be calculatet with degree
        println!("klar med sleep!");
        self.stop_vehicle();
    }
    //Rotate vehicle X amount degrees right
    pub fn inst_rotate_r(&mut self, degrees: i32) {
        println!("Rotating {degrees} degrees right!");
        self.rotation(75, false);
        sleep(Duration::from_millis(self.calculate_degree_sleep(degrees))); // This will be calculatet with degree
        self.stop_vehicle();
    }

    fn calculate_degree_sleep(&mut self, degrees: i32) -> u64{
        let time_per_degree:f32 = 2.0/180.0;
        let time_to_spin = time_per_degree*(degrees as f32);
        let millis_time: u64 = (time_to_spin*(1000 as f32)) as u64;
        println!("sleep tid rotation: {millis_time}");
        millis_time
    }
    ///Forward X meters
    ///2,72
    ///2,82
    ///2,81
    ///2,80
    pub fn inst_forward(&mut self, meters: i32) {
        println!("Driving forward {meters} meters!");
        let speed: u8 = self.calculate_speed(75) as u8;
        self.forward(speed);
        let calc: u64 = (2800*meters).try_into().unwrap();
        sleep(Duration::from_millis(calc)); // This will be calculatet with meter
        self.stop_vehicle();
    }
    ///Backwards X meters
    ///2,8
    pub fn inst_backward(&mut self, meters: i32) {
        println!("Driving backward {meters} meters!");
        let speed: u8 = self.calculate_speed(75) as u8;
        self.backwards(speed);        
        let calc:u64 = (2800*meters).try_into().unwrap();
        sleep(Duration::from_millis(calc)); // This will be calculatet with meter
        self.stop_vehicle();
    }
    ///function for handling rotations.
    /// 180 grader 2 sekunder. 90 grader 1 sekund Båda sidor!.
    ///
    fn rotation(&mut self, speed: i32, left: bool) {
        let speed: u8 = self.calculate_speed(speed) as u8;
        if left {
            self.fr_wheel(speed, 0);
            self.br_wheel(speed, 0);
            self.fl_wheel(0, speed);
            self.bl_wheel(0, speed);
        } else {
            self.fr_wheel(0, speed);
            self.br_wheel(0, speed);
            self.fl_wheel(speed, 0);
            self.bl_wheel(speed, 0);
        }
    }
    //---------------------------------------------
}

///Different addresses for i2c interdace. Can be found in data sheet.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DeviceAddr {
    DEFADR = 0x15,
    SFTRESET = 0x03,
}

impl DeviceAddr {
    fn address(&self) -> u8 {
        *self as u8
    }
}

///different registers for PCA9634. Can be found in data sheet.
#[derive(Clone, Copy)]
pub enum Register {
    MODE1 = 0x00,
    MODE2 = 0x01,
    PWM0 = 0x02,
    PWM1 = 0x03,
    PWM2 = 0x04,
    PWM3 = 0x05,
    PWM4 = 0x06,
    PWM5 = 0x07,
    PWM6 = 0x08,
    PWM7 = 0x09,
    GRPPWM = 0x0A,
    GRPFREQ = 0x0B,
    LEDOUT0 = 0x0C,
    LEDOUT1 = 0x0D,
    SUBADR1 = 0x0E,
    SUBADR2 = 0x0F,
    SUBADR3 = 0x10,
    ALLCALLADR = 0x11,
}

///Functions to print out name, also to get register adress of an unsigned 8-bit address.
impl Register {
    fn address(&self) -> u8 {
        *self as u8
    }

    fn to_string(&self) -> &str {
        match self {
            Register::MODE1 => "MODE1",
            Register::MODE2 => "MODE2",
            Register::PWM0 => "PWM0",
            Register::PWM1 => "PWM1",
            Register::PWM2 => "PWM2",
            Register::PWM3 => "PWM3",
            Register::PWM4 => "PWM4",
            Register::PWM5 => "PWM5",
            Register::PWM6 => "PWM6",
            Register::PWM7 => "PWM7",
            Register::GRPPWM => "GRPPWM",
            Register::GRPFREQ => "GRPFREQ",
            Register::LEDOUT0 => "LEDOUT0",
            Register::LEDOUT1 => "LEDOUT1",
            Register::SUBADR1 => "SUBADR1",
            Register::SUBADR2 => "SUBADR2",
            Register::SUBADR3 => "SUBADR3",
            Register::ALLCALLADR => "ALLCALLADR",
        }
    }
}
