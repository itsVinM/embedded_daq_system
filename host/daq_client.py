#!/usr/bin/env python3
"""
DAQ Host Client - Python UART interface for embedded_daq_system

Receives SamplePacket data from STM32F401RE over USART2 (PA2/PA3)
Parses binary packets and displays ADC measurements in real-time.
"""

import argparse
import serial
import struct
import sys
import time
from datetime import datetime

# SamplePacket structure constants
PACKET_MAGIC = 0xDA71
SAMPLE_COUNT_BYTES = 2  # u16 for count
MAGIC_BYTES = 2          # u16 for magic
SEQ_BYTES = 2            # u16 for sequence

# Sample structure
# channel: u8, timestamp_us: u32, value: u32, flags: u8
SAMPLE_SIZE = 1 + 4 + 4 + 1  # 10 bytes per sample
MAX_SAMPLES = 32
PACKET_SIZE = (MAGIC_BYTES + SEQ_BYTES + SAMPLE_COUNT_BYTES +
              MAX_SAMPLES * SAMPLE_SIZE)

# Byte order for binary parsing
BYTE_ORDER = '<'  # Little-endian

class Sample:
    """Parsed sample data from DAQ firmware."""
    def __init__(self, channel: int, timestamp_us: int, value: int, flags: int):
        self.channel = channel
        self.timestamp_us = timestamp_us
        self.value = value
        self.flags = flags

    @classmethod
    def from_bytes(cls, data: bytes):
        """Parse sample from 10-byte chunk."""
        if len(data) != SAMPLE_SIZE:
            raise ValueError(f"Sample data must be {SAMPLE_SIZE} bytes, got {len(data)}")
        
        channel, = struct.unpack(f'{BYTE_ORDER}B', data[0:1])
        timestamp, = struct.unpack(f'{BYTE_ORDER}I', data[1:5])
        value, = struct.unpack(f'{BYTE_ORDER}I', data[5:9])
        flags, = struct.unpack(f'{BYTE_ORDER}B', data[9:10])
        
        return cls(channel, timestamp, value, flags)

    def channel_name(self):
        """Human-readable channel name."""
        names = {0: "Analog0", 1: "Analog1", 2: "PWM In"}
        return names.get(self.channel, f"Channel{self.channel}")

    def to_mv(self, vref_mv: int = 3300) -> int:
        """Convert ADC value to millivolts."""
        return (self.value * vref_mv) // 4095

    def __str__(self):
        mv = self.to_mv()
        return (f"{self.channel_name()}: {self.value} counts ({mv} mV) "
                f"@ {self.timestamp_us} us")

    def to_json(self):
        return {
            'channel': self.channel,
            'timestamp_us': self.timestamp_us,
            'value': self.value,
            'value_mv': self.to_mv(),
            'flags': self.flags
        }

class SamplePacket:
    """Parsed SamplePacket from DAQ firmware."""
    def __init__(self, magic: int, seq: int, count: int, samples: list):
        self.magic = magic
        self.seq = seq
        self.count = count
        self.samples = samples

    @classmethod
    def from_bytes(cls, data: bytes):
        """Parse complete packet from raw bytes."""
        if len(data) < MAGIC_BYTES + SEQ_BYTES + SAMPLE_COUNT_BYTES:
            raise ValueError("Packet too short to contain header")
        
        magic, seq, count = struct.unpack(
            f'{BYTE_ORDER}HHH', 
            data[0:MAGIC_BYTES + SEQ_BYTES + SAMPLE_COUNT_BYTES]
        )
        
        if magic != PACKET_MAGIC:
            raise ValueError(f"Invalid packet magic: 0x{magic:04X}, expected 0x{PACKET_MAGIC:04X}")
        
        if count > MAX_SAMPLES:
            raise ValueError(f"Packet claims {count} samples, max is {MAX_SAMPLES}")
        
        sample_data = data[MAGIC_BYTES + SEQ_BYTES + SAMPLE_COUNT_BYTES:]
        if len(sample_data) < count * SAMPLE_SIZE:
            raise ValueError(f"Packet claims {count} samples but only has {len(sample_data)//SAMPLE_SIZE} samples worth of data")
        
        samples = []
        for i in range(count):
            start = i * SAMPLE_SIZE
            end = start + SAMPLE_SIZE
            samples.append(Sample.from_bytes(sample_data[start:end]))
        
        return cls(magic, seq, count, samples)

    def is_valid(self) -> bool:
        """Check if packet is valid."""
        return (
            self.magic == PACKET_MAGIC and
            0 < self.count <= MAX_SAMPLES and
            len(self.samples) == self.count
        )

    def __str__(self):
        samples_str = '\n'.join(
            f"  [{i}] {sample}" for i, sample in enumerate(self.samples)
        )
        return (f"Packet #{self.seq} (valid: {self.is_valid()}, "
                f"count: {self.count}):\n{samples_str}")

    def to_dict(self):
        """Convert packet to dictionary for JSON serialization."""
        return {
            'magic': self.magic,
            'seq': self.seq,
            'count': self.count,
            'samples': [s.to_dict() for s in self.samples]
        }

class DAQHostClient:
    """Main client class for receiving data from DAQ firmware."""
    
    def __init__(self, port: str, baudrate: int = 115200, debug: bool = False):
        self.port = port
        self.baudrate = baudrate
        self.debug = debug
        self.serial = None
        self.last_seq = -1
        self.packet_count = 0
        self.error_count = 0

    def connect(self) -> bool:
        """Connect to the DAQ firmware."""
        try:
            self.serial = serial.Serial(
                port=self.port,
                baudrate=self.baudrate,
                timeout=1.0,
                write_timeout=1.0
            )
            self.serial.flushInput()
            self.serial.flushOutput()
            print(f"Connected to {self.port} at {self.baudrate} baud")
            return True
        except serial.SerialException as e:
            print(f"Failed to connect to {self.port}: {e}")
            return False

    def disconnect(self):
        """Disconnect from the DAQ firmware."""
        if self.serial:
            self.serial.close()
            self.serial = None

    def wait_for_packet(self, max_wait: float = 5.0) -> SamplePacket:
        """Wait for and parse a complete packet."""
        start_time = time.time()
        buffer = bytearray()
        
        while time.time() - start_time < max_wait:
            if self.serial.in_waiting > 0:
                # Read available bytes
                data = self.serial.read(self.serial.in_waiting)
                buffer.extend(data)
                
                # Try to parse packets from the buffer
                offset = 0
                while offset + PACKET_SIZE <= len(buffer):
                    try:
                        packet = SamplePacket.from_bytes(buffer[offset:offset + PACKET_SIZE])
                        if packet.is_valid():
                            # Remove parsed packet from buffer
                            del buffer[offset:offset + PACKET_SIZE]
                            return packet
                    except (ValueError, struct.error) as e:
                        # Skip invalid data at beginning of buffer
                        if self.debug:
                            print(f"Invalid data (offset {offset}): {e}")
                        del buffer[offset:offset + 1]
                        continue
            
            time.sleep(0.01)
        
        raise TimeoutError(f"Timeout waiting for valid packet (waited {max_wait}s)")

    def run(self):
        """Main run loop."""
        print("Starting DAQ Host Client...")
        print(f"Protocol: 0x{PACKET_MAGIC:04X}, Packet size: {PACKET_SIZE} bytes")
        print("Press Ctrl+C to stop")
        
        if not self.connect():
            return 1
        
        try:
            while True:
                try:
                    packet = self.wait_for_packet()
                    
                    # Validate sequence number
                    if packet.seq <= self.last_seq:
                        print(f"Warning: Sequence number {packet.seq} <= {self.last_seq}, possible packet loss")
                    self.last_seq = packet.seq
                    
                    self.packet_count += 1
                    timestamp = datetime.now().strftime("%H:%M:%S.%f")[:-3]
                    
                    print(f"\n[{timestamp}] Packet #{packet.seq} (count: {packet.count}):")
                    for i, sample in enumerate(packet.samples):
                        print(f"  [{i}] {sample}")
                    
                    # Print packet-level summary
                    analog0_samples = [s for s in packet.samples if s.channel == 0]
                    analog1_samples = [s for s in packet.samples if s.channel == 1]
                    pwm_samples = [s for s in packet.samples if s.channel == 2]
                    
                    if analog0_samples:
                        avg_mv = sum(s.to_mv() for s in analog0_samples) // len(analog0_samples)
                        print(f"  Analog0 avg: ~{avg_mv} mV ({len(analog0_samples)} samples)")
                    
                    if analog1_samples:
                        avg_mv = sum(s.to_mv() for s in analog1_samples) // len(analog1_samples)
                        print(f"  Analog1 avg: ~{avg_mv} mV ({len(analog1_samples)} samples)")
                    
                    if pwm_samples:
                        print(f"  PWM Input: {len(pwm_samples)} samples")
                
                except (TimeoutError, serial.SerialException) as e:
                    self.error_count += 1
                    print(f"\nError: {e}")
                    if self.error_count > 5:
                        print("Too many consecutive errors, disconnecting")
                        break
        
        except KeyboardInterrupt:
            print("\nStopping...")
        
        finally:
            self.disconnect()
        
        print(f"\nSummary: {self.packet_count} packets received, {self.error_count} errors")
        return 0

def main():
    parser = argparse.ArgumentParser(
        description="DAQ Host Client - Receive data from embedded_daq_system"
    )
    parser.add_argument(
        'port',
        help='Serial port (e.g., COM3 on Windows, /dev/ttyACM0 on Linux)'
    )
    parser.add_argument(
        '-b', '--baudrate',
        type=int,
        default=115200,
        help='Baud rate (default: 115200)'
    )
    parser.add_argument(
        '-d', '--debug',
        action='store_true',
        help='Enable debug output'
    )
    
    args = parser.parse_args()
    
    client = DAQHostClient(args.port, args.baudrate, args.debug)
    exit_code = client.run()
    sys.exit(exit_code)

if __name__ == "__main__":
    main()