use defmt::info;
use shared::{TelemetryReader, TelemetryFrame, TelemetryChannel};

pub struct VoltageSensor {
    pub channel: TelemetryChannel,
    pub offset_mv: i32,
    pub gain: i32,
}

impl VoltageSensor {
    pub fn new(channel: TelemetryChannel) -> Self {
        Self { channel, offset_mv: 0, gain: 4095 }
    }

    pub fn with_calibration(channel: TelemetryChannel, offset_mv: i32, gain: i32) -> Self {
        Self { channel, offset_mv, gain }
    }
}

pub struct CurrentSensor {
    pub channel: TelemetryChannel,
    pub shunt_mohm: u32,
}

pub struct TemperatureSensor {
    pub channel: TelemetryChannel,
    pub offset_celsius: i32,
}

pub struct RtcReader {
    pub uptime_us: u32,
}

pub struct AdcBackend {
    pub raw_values: [u32; 4],
}

impl<'d> TelemetryReader<'d, VoltageSensor> for AdcBackend {
    type Error = ();

    fn read_raw(&mut self, sensor: &'d VoltageSensor) -> Result<u32, ()> {
        let idx = sensor.channel as usize;
        if idx < self.raw_values.len() {
            Ok(self.raw_values[idx])
        } else {
            Err(())
        }
    }

    fn read_calibrated(&mut self, sensor: &'d VoltageSensor) -> Result<TelemetryFrame, ()> {
        let raw = self.read_raw(sensor)?;
        let mv = ((raw as i32 - sensor.offset_mv) * 3300) / sensor.gain;
        Ok(TelemetryFrame {
            channel: sensor.channel,
            timestamp_us: 0,
            value_i32: mv,
            value_u32: mv as u32,
            flags: 0,
        })
    }

    fn channel(&self) -> TelemetryChannel {
        TelemetryChannel::Voltage
    }

    fn last_reading(&self) -> Option<TelemetryFrame> {
        None
    }
}

impl<'d> TelemetryReader<'d, CurrentSensor> for AdcBackend {
    type Error = ();

    fn read_raw(&mut self, sensor: &'d CurrentSensor) -> Result<u32, ()> {
        let idx = sensor.channel as usize;
        if idx < self.raw_values.len() {
            Ok(self.raw_values[idx])
        } else {
            Err(())
        }
    }

    fn read_calibrated(&mut self, sensor: &'d CurrentSensor) -> Result<TelemetryFrame, ()> {
        let raw = self.read_raw(sensor)?;
        let voltage_uv = (raw as i32 * 3300_000) / 4095;
        let current_ua = if sensor.shunt_mohm > 0 {
            voltage_uv * 1000 / sensor.shunt_mohm as i32
        } else {
            0
        };
        Ok(TelemetryFrame {
            channel: sensor.channel,
            timestamp_us: 0,
            value_i32: current_ua,
            value_u32: current_ua as u32,
            flags: 0,
        })
    }

    fn channel(&self) -> TelemetryChannel {
        TelemetryChannel::Current
    }

    fn last_reading(&self) -> Option<TelemetryFrame> {
        None
    }
}

impl<'d> TelemetryReader<'d, TemperatureSensor> for AdcBackend {
    type Error = ();

    fn read_raw(&mut self, sensor: &'d TemperatureSensor) -> Result<u32, ()> {
        let idx = sensor.channel as usize;
        if idx < self.raw_values.len() {
            Ok(self.raw_values[idx])
        } else {
            Err(())
        }
    }

    fn read_calibrated(&mut self, sensor: &'d TemperatureSensor) -> Result<TelemetryFrame, ()> {
        let raw = self.read_raw(sensor)?;
        let temp_c = ((raw as i32 * 3300) / 4095 - 760) * 10 / 25 + sensor.offset_celsius;
        Ok(TelemetryFrame {
            channel: sensor.channel,
            timestamp_us: 0,
            value_i32: temp_c,
            value_u32: temp_c as u32,
            flags: 0,
        })
    }

    fn channel(&self) -> TelemetryChannel {
        TelemetryChannel::Temperature
    }

    fn last_reading(&self) -> Option<TelemetryFrame> {
        None
    }
}

impl<'d> TelemetryReader<'d, RtcReader> for AdcBackend {
    type Error = ();

    fn read_raw(&mut self, _sensor: &'d RtcReader) -> Result<u32, ()> {
        Ok(0)
    }

    fn read_calibrated(&mut self, sensor: &'d RtcReader) -> Result<TelemetryFrame, ()> {
        Ok(TelemetryFrame {
            channel: TelemetryChannel::Rtc,
            timestamp_us: sensor.uptime_us,
            value_i32: sensor.uptime_us as i32,
            value_u32: sensor.uptime_us,
            flags: 0,
        })
    }

    fn channel(&self) -> TelemetryChannel {
        TelemetryChannel::Rtc
    }

    fn last_reading(&self) -> Option<TelemetryFrame> {
        None
    }
}

#[embassy_executor::task]
pub async fn telemetry_task() {
    use shared::TelemetryBus;
    let mut bus = TelemetryBus::<8>::new();
    let mut backend = AdcBackend { raw_values: [0; 4] };
    let voltage = VoltageSensor::new(TelemetryChannel::Voltage);
    let temp = TemperatureSensor::new(TelemetryChannel::Temperature);

    info!("telemetry: task started");

    loop {
        if let Ok(frame) = backend.read_calibrated(&voltage) {
            bus.push(frame);
        }
        if let Ok(frame) = backend.read_calibrated(&temp) {
            bus.push(frame);
        }

        if bus.is_full() {
            let frames = bus.drain();
            for frame in frames {
                info!("telemetry: ch={} val={}", frame.channel as u8, frame.value_i32);
            }
        }

        embassy_time::Timer::after_millis(100).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn voltage_sensor_read() {
        let mut backend = AdcBackend { raw_values: [2048, 0, 0, 0] };
        let sensor = VoltageSensor::new(TelemetryChannel::Voltage);
        let raw = backend.read_raw(&sensor).unwrap();
        assert_eq!(raw, 2048);
    }

    #[test]
    fn voltage_calibration() {
        let mut backend = AdcBackend { raw_values: [2048, 0, 0, 0] };
        let sensor = VoltageSensor::new(TelemetryChannel::Voltage);
        let frame = backend.read_calibrated(&sensor).unwrap();
        assert_eq!(frame.value_i32, 1650);
        assert_eq!(frame.channel, TelemetryChannel::Voltage);
    }

    #[test]
    fn current_sensor_read() {
        let mut backend = AdcBackend { raw_values: [0, 1024, 0, 0] };
        let sensor = CurrentSensor { channel: TelemetryChannel::Current, shunt_mohm: 100 };
        let frame = backend.read_calibrated(&sensor).unwrap();
        assert!(frame.value_i32 > 0);
    }

    #[test]
    fn temperature_sensor_read() {
        let mut backend = AdcBackend { raw_values: [0, 0, 500, 0] };
        let sensor = TemperatureSensor { channel: TelemetryChannel::Temperature, offset_celsius: 0 };
        let frame = backend.read_calibrated(&sensor).unwrap();
        assert_eq!(frame.channel, TelemetryChannel::Temperature);
    }

    #[test]
    fn out_of_range_channel_returns_error() {
        let mut backend = AdcBackend { raw_values: [0; 4] };
        let sensor = VoltageSensor {
            channel: TelemetryChannel::Gpio,
            offset_mv: 0,
            gain: 4095,
        };
        assert!(backend.read_raw(&sensor).is_err());
    }
}
