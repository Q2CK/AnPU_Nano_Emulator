use std::{env,
          thread::sleep,
          time::{self, Duration},
          io::{stdout, Write}};

use crossterm::{ExecutableCommand,
                QueueableCommand,
                terminal::{self, SetSize, enable_raw_mode},
                cursor::{self, MoveTo},
                style::{self, Stylize, Color, PrintStyledContent, Attribute, Print, SetAttribute, SetAttributes, SetBackgroundColor, SetForegroundColor},
                event::{read, poll, Event},
                Result};
use crossterm::event::{KeyCode, KeyEventKind};
use crossterm::style::Attributes;
use crate::Mode::{Automatic, ManualStep, Setup};

const WINDOW_SIZE: (u16, u16) = (65, 24);

const BG_COLOR: Color = Color::Black;
const FIELD_COLOR: Color = Color::Black;


#[derive(Debug)]
enum Mode {
    Setup,
    ManualStep,
    Automatic(u16)
}

struct EmulatorState {
    rom: [u32; 64],
    ram: [u16; 32],
    reg: [u16; 8],
    inp: [u16; 8],
    out: [u16; 8],
    flg: [bool; 16],
    pc: u16,

    mode: Mode,
    log_buffer: [String; 7],

    current_rom_read: Option<u16>,
    current_ram_write: Option<u16>,
    current_reg_write: Option<u16>,
}

impl EmulatorState {
    fn full_reset(&mut self) -> Result<()> {
        self.rom = [0; 64];
        self.program_reset()?;

        Ok(())
    }

    fn program_reset(&mut self) -> Result<()> {
        self.ram = [0; 32];
        self.reg = [0; 8];
        self.inp = [0; 8];
        self.out = [0; 8];
        self.flg = [false; 16];
        self.pc = 0;

        self.mode = Setup;
        self.log_buffer = Default::default();

        self.reset_last_mods();
        self.draw_contents()?;

        Ok(())
    }

    fn reset_last_mods(&mut self){
        let mut stdout = stdout();

        if let Some(i) = self.current_rom_read {
            let i = i % 64;
            let value = self.rom[i as usize] % 65536;
            let hex = &format!("{value:x}");
            stdout.queue(MoveTo((5 * (i % 8) + 6) as u16, (i / 8 + 3) as u16)).unwrap();
            stdout.queue(PrintStyledContent(format!("{hex:0>0$}", 4).white())).unwrap();
        }

        if let Some(i) = self.current_ram_write {
            let i = i % 32;
            let value = self.ram[i as usize] % 256;
            let hex = &format!("{value:x}");
            stdout.queue(MoveTo((3 * (i % 4) + 52) as u16, (i / 4 + 3) as u16)).unwrap();
            stdout.queue(PrintStyledContent(format!("{hex:0>0$}", 2).white())).unwrap();
        }

        if let Some(i) = self.current_reg_write {
            let i = i % 8;
            let val = self.reg[i as usize];
            let hex = &format!("{val:x}");
            stdout.queue(MoveTo(6, 13 + i)).unwrap();
            stdout.queue(PrintStyledContent(format!("{hex:0>0$}", 2).white())).unwrap();
        }

        self.current_rom_read = None;
        self.current_ram_write = None;
        self.current_reg_write = None;
    }

    fn draw_contents(&self) -> Result<()> {
        let mut stdout = stdout();

        for idx in 0..64 {
            let value = self.rom[idx as usize] % 65536;
            let hex = &format!("{value:x}");
            stdout.queue(MoveTo((5 * (idx % 8) + 6) as u16, (idx / 8 + 3) as u16))?;
            stdout.queue(PrintStyledContent(format!("{hex:0>0$}", 4).white()))?;
        }
        for idx in 0..32 {
            let value = self.ram[idx as usize] % 256;
            let hex = &format!("{value:x}");
            stdout.queue(MoveTo((3 * (idx % 4) + 52) as u16, (idx / 4 + 3) as u16))?;
            stdout.queue(PrintStyledContent(format!("{hex:0>0$}", 2).white()))?;
        }
        for idx in 0..8 {
            let val = self.reg[idx as usize];
            let hex = &format!("{val:x}");
            stdout.queue(MoveTo(6, 13 + idx))?;
            stdout.queue(PrintStyledContent(format!("{hex:0>0$}", 2).white()))?;
            let val = self.inp[idx as usize];
            let hex = &format!("{val:x}");
            stdout.queue(MoveTo(15, 13 + idx))?;
            stdout.queue(PrintStyledContent(format!("{hex:0>0$}", 2).white()))?;
            let val = self.out[idx as usize];
            let hex = &format!("{val:x}");
            stdout.queue(MoveTo(24, 13 + idx))?;
            stdout.queue(PrintStyledContent(format!("{hex:0>0$}", 2).white()))?;
        }
        for idx in 0..16 {
            stdout.queue(MoveTo((idx / 8) as u16 * 5 + 32, idx % 8 + 13))?;
            let value = match self.flg[idx as usize] {
                true => 'T',
                false => 'F'
            };
            stdout.queue(PrintStyledContent(value.white()))?;
        }

        if let Some(i) = self.current_rom_read {
            let i = i % 64;
            let value = self.rom[i as usize] % 65536;
            let hex = &format!("{value:x}");
            stdout.queue(MoveTo((5 * (i % 8) + 6) as u16, (i / 8 + 3) as u16))?;
            stdout.queue(PrintStyledContent(format!("{hex:0>0$}", 4).green()))?;
        }

        if let Some(i) = self.current_ram_write {
            let i = i % 32;
            let value = self.ram[i as usize] % 256;
            let hex = &format!("{value:x}");
            stdout.queue(MoveTo((3 * (i % 4) + 52) as u16, (i / 4 + 3) as u16))?;
            stdout.queue(PrintStyledContent(format!("{hex:0>0$}", 2).green()))?;
        }

        if let Some(i) = self.current_reg_write {
            let i = i % 8;
            let val = self.reg[i as usize];
            let hex = &format!("{val:x}");
            stdout.queue(MoveTo(6, 13 + i))?;
            stdout.queue(PrintStyledContent(format!("{hex:0>0$}", 2).green()))?;
        }

        Ok(())
    }

    fn write_to_rom(&mut self, idx: u16, val: u32) -> Result<()> {
        let mut stdout = stdout();

        let idx = idx % 64;
        self.rom[idx as usize] = val % 65536;
        let value = val % 65536;
        let hex = &format!("{value:x}");
        stdout.queue(MoveTo((5 * (idx % 8) + 6) as u16, (idx / 8 + 3) as u16))?;
        stdout.queue(PrintStyledContent(format!("{hex:0>0$}", 4).white()))?;

        Ok(())
    }

    fn read_from_rom(&mut self, idx: u16) -> u32 {
        let mut stdout = stdout();

        if let Some(i) = self.current_rom_read {
            let i = i % 64;
            let value = self.rom[i as usize] % 65536;
            let hex = &format!("{value:x}");
            stdout.queue(MoveTo((5 * (i % 8) + 6) as u16, (i / 8 + 3) as u16)).unwrap();
            stdout.queue(PrintStyledContent(format!("{hex:0>0$}", 4).white())).unwrap();
        }

        let idx = idx % 64;
        let val = self.rom[idx as usize] % 65536;
        let hex = &format!("{val:x}");
        stdout.queue(MoveTo((5 * (idx % 8) + 6) as u16, (idx / 8 + 3) as u16)).unwrap();
        stdout.queue(PrintStyledContent(format!("{hex:0>0$}", 4).green())).unwrap();

        self.current_rom_read = Some(idx);
        return self.rom[idx as usize];
    }

    fn write_to_ram(&mut self, idx: u16, val: u16) -> Result<()> {
        let mut stdout = stdout();

        if let Some(i) = self.current_ram_write {
            let i = i % 32;
            let value = self.ram[i as usize] % 256;
            let hex = &format!("{value:x}");
            stdout.queue(MoveTo((3 * (i % 4) + 52) as u16, (i / 4 + 3) as u16))?;
            stdout.queue(PrintStyledContent(format!("{hex:0>0$}", 2).white()))?;
        }

        let idx = idx % 32;
        self.ram[idx as usize] = val % 256;
        let val = val % 256;
        let hex = &format!("{val:x}");
        stdout.queue(MoveTo((3 * (idx % 4) + 52) as u16, (idx / 4 + 3) as u16))?;
        stdout.queue(PrintStyledContent(format!("{hex:0>0$}", 2).green()))?;

        self.current_ram_write = Some(idx);

        Ok(())
    }

    fn write_to_regs(&mut self, idx: u16, val: u16) -> Result<()> {
        let mut stdout = stdout();

        if let Some(i) = self.current_reg_write {
            let i = i % 8;
            let val = self.reg[i as usize];
            let hex = &format!("{val:x}");
            stdout.queue(MoveTo(6, 13 + i))?;
            stdout.queue(PrintStyledContent(format!("{hex:0>0$}", 2).white()))?;
        }

        let idx = idx % 8;
        self.reg[idx as usize] = val % 256;
        let hex = &format!("{val:x}");
        stdout.queue(MoveTo(6, 13 + idx))?;
        stdout.queue(PrintStyledContent(format!("{hex:0>0$}", 2).green()))?;

        self.current_reg_write = Some(idx);

        Ok(())
    }

    fn draw_mode(&mut self) -> Result<()> {
        let mut stdout = stdout();

        stdout.queue(MoveTo(57, 12))?;
        match self.mode {
            Setup => stdout.queue(PrintStyledContent("HALTED".red()))?,
            ManualStep => stdout.queue(PrintStyledContent("MANUAL".yellow()))?,
            Automatic(speed) => stdout.queue(PrintStyledContent(format!("{speed:->0$}", 6).green()))?
        };

        Ok(())
    }

    fn draw_help(&mut self) -> Result<()> {
        let mut stdout = stdout();

        match self.mode {
            Setup => {
                stdout.queue(MoveTo(2, 22))?;
                stdout.queue(PrintStyledContent("L".cyan()))?;
                stdout.queue(PrintStyledContent(" - load program ".white()))?;
                stdout.queue(PrintStyledContent("C".cyan()))?;
                stdout.queue(PrintStyledContent(" - clear ".white()))?;
                stdout.queue(PrintStyledContent("R".cyan()))?;
                stdout.queue(PrintStyledContent(" - run ".white()))?;
                stdout.queue(PrintStyledContent("S".cyan()))?;
                stdout.queue(PrintStyledContent(" - step                   ".white()))?;
            }
            ManualStep | Automatic(_) => {
                stdout.queue(MoveTo(2, 22))?;
                stdout.queue(PrintStyledContent(" ".cyan()))?;
                stdout.queue(PrintStyledContent("                ".white()))?;
                stdout.queue(PrintStyledContent("C".cyan()))?;
                stdout.queue(PrintStyledContent(" - clear ".white()))?;
                stdout.queue(PrintStyledContent(" ".cyan()))?;
                stdout.queue(PrintStyledContent("       ".white()))?;
                stdout.queue(PrintStyledContent("S".cyan()))?;
                stdout.queue(PrintStyledContent(" - step                   ".white()))?;
            }
        }

        Ok(())
    }

    fn draw_layout(&mut self) -> Result<()> {
        let mut stdout = stdout();

        stdout.queue(SetBackgroundColor(BG_COLOR))?;

        for i in 0..WINDOW_SIZE.0 {
            for j in 0..WINDOW_SIZE.1 {
                stdout.queue(MoveTo(i, j))?;
                stdout.queue(Print(" "))?;
            }
        }

        if let Some(i) = self.current_rom_read {
            let i = i % 64;
            let value = self.rom[i as usize] % 65536;
            let hex = &format!("{value:x}");
            stdout.queue(MoveTo((5 * (i % 8) + 6) as u16, (i / 8 + 3) as u16)).unwrap();
            stdout.queue(PrintStyledContent(format!("{hex:0>0$}", 4).white())).unwrap();
        }

        if let Some(i) = self.current_ram_write {
            let i = i % 32;
            let value = self.ram[i as usize] % 256;
            let hex = &format!("{value:x}");
            stdout.queue(MoveTo((3 * (i % 4) + 52) as u16, (i / 4 + 3) as u16)).unwrap();
            stdout.queue(PrintStyledContent(format!("{hex:0>0$}", 2).white())).unwrap();
        }

        if let Some(i) = self.current_reg_write {
            let i = i % 8;
            let val = self.reg[i as usize];
            stdout.queue(MoveTo(6, 13 + i)).unwrap();
            let hex = &format!("{val:x}");
            stdout.queue(PrintStyledContent(format!("{hex:0>0$}", 2).white())).unwrap();
        }

        stdout.execute(MoveTo(0, 0))?;
        stdout.execute(SetBackgroundColor(Color::Magenta))?;
        stdout.execute(SetAttribute(Attribute::Bold))?;
        stdout.execute(SetAttribute(Attribute::Underlined))?;
        stdout.execute(PrintStyledContent(" AnPU Nano emulator                                         Q2CK ".white()))?;
        stdout.execute(SetAttribute(Attribute::Reset))?;


        draw_box((0, 1), (47, 11), "".to_string())?;
        stdout.queue(SetBackgroundColor(FIELD_COLOR))?;
        stdout.execute(SetAttribute(Attribute::Bold))?;
        stdout.execute(SetAttribute(Attribute::Underlined))?;
        stdout.queue(MoveTo(2, 2))?;
        stdout.queue(PrintStyledContent("ROM".magenta()))?;
        stdout.execute(SetAttribute(Attribute::Reset))?;
        stdout.queue(SetBackgroundColor(FIELD_COLOR))?;
        stdout.queue(MoveTo(6, 2))?;
        stdout.queue(PrintStyledContent(" 000  001  010  011  100  101  110  111".cyan()))?;
        for i in 0..8 {
            stdout.queue(MoveTo(2, 3 + i))?;

            let bin = format!("{i:b}");
            stdout.queue(PrintStyledContent(format!("{bin:0>0$}", 3).cyan()))?;
        }

        draw_box((46, 1), (29, 11), "".to_string())?;
        stdout.queue(SetBackgroundColor(FIELD_COLOR))?;
        stdout.execute(SetAttribute(Attribute::Bold))?;
        stdout.execute(SetAttribute(Attribute::Underlined))?;
        stdout.queue(MoveTo(48, 2))?;
        stdout.queue(PrintStyledContent("RAM".magenta()))?;
        stdout.execute(SetAttribute(Attribute::Reset))?;
        stdout.queue(SetBackgroundColor(FIELD_COLOR))?;
        stdout.queue(MoveTo(52, 2))?;
        stdout.queue(PrintStyledContent("00 01 10 11".cyan()))?;
        for i in 0..8 {
            stdout.queue(MoveTo(48, 3 + i))?;
            let bin = format!("{i:b}");
            stdout.queue(PrintStyledContent(format!("{bin:0>0$}", 3).cyan()))?;
        }

        draw_box((0, 11), (10, 11), "".to_string())?;
        stdout.queue(SetBackgroundColor(FIELD_COLOR))?;
        stdout.execute(SetAttribute(Attribute::Bold))?;
        stdout.execute(SetAttribute(Attribute::Underlined))?;
        stdout.queue(MoveTo(2, 12))?;
        stdout.queue(PrintStyledContent("REG".magenta()))?;
        stdout.execute(SetAttribute(Attribute::Reset))?;
        stdout.queue(SetBackgroundColor(FIELD_COLOR))?;
        for i in 0..8 {
            stdout.queue(MoveTo(2, 13 + i))?;
            let bin = format!("{i:b}");
            stdout.queue(PrintStyledContent(format!("{bin:0>0$} ", 3).cyan()))?;
        }

        draw_box((9, 11), (10, 11), "".to_string())?;
        stdout.queue(SetBackgroundColor(FIELD_COLOR))?;
        stdout.execute(SetAttribute(Attribute::Bold))?;
        stdout.execute(SetAttribute(Attribute::Underlined))?;
        stdout.queue(MoveTo(11, 12))?;
        stdout.queue(PrintStyledContent("INP".magenta()))?;
        stdout.execute(SetAttribute(Attribute::Reset))?;
        stdout.queue(SetBackgroundColor(FIELD_COLOR))?;
        for i in 0..8 {
            let bin = format!("{i:b}");
            stdout.queue(MoveTo(11, 13 + i))?;
            stdout.queue(PrintStyledContent(format!("{bin:0>0$} ", 3).cyan()))?;
        }

        draw_box((18, 11), (10, 11), "".to_string())?;
        stdout.queue(SetBackgroundColor(FIELD_COLOR))?;
        stdout.execute(SetAttribute(Attribute::Bold))?;
        stdout.execute(SetAttribute(Attribute::Underlined))?;
        stdout.queue(MoveTo(20, 12))?;
        stdout.queue(PrintStyledContent("OUT".magenta()))?;
        stdout.execute(SetAttribute(Attribute::Reset))?;
        stdout.queue(SetBackgroundColor(FIELD_COLOR))?;
        for i in 0..8 {
            let bin = format!("{i:b}");
            stdout.queue(MoveTo(20, 13 + i))?;
            stdout.queue(PrintStyledContent(format!("{bin:0>0$} ", 3).cyan()))?;
        }

        draw_box((27, 11), (13, 11), "".to_string())?;
        stdout.queue(SetBackgroundColor(FIELD_COLOR))?;
        stdout.execute(SetAttribute(Attribute::Bold))?;
        stdout.execute(SetAttribute(Attribute::Underlined))?;
        stdout.queue(MoveTo(29, 12))?;
        stdout.queue(PrintStyledContent("FLG".magenta()))?;
        stdout.execute(SetAttribute(Attribute::Reset))?;
        stdout.queue(MoveTo(29, 13))?;
        stdout.queue(PrintStyledContent("ZE".cyan()))?;
        stdout.queue(MoveTo(29, 14))?;
        stdout.queue(PrintStyledContent("NZ".cyan()))?;
        stdout.queue(MoveTo(29, 15))?;
        stdout.queue(PrintStyledContent("CA".cyan()))?;
        stdout.queue(MoveTo(29, 16))?;
        stdout.queue(PrintStyledContent("NC".cyan()))?;
        stdout.queue(MoveTo(29, 17))?;
        stdout.queue(PrintStyledContent("OF".cyan()))?;
        stdout.queue(MoveTo(29, 18))?;
        stdout.queue(PrintStyledContent("NO".cyan()))?;
        stdout.queue(MoveTo(29, 19))?;
        stdout.queue(PrintStyledContent("EV".cyan()))?;
        stdout.queue(MoveTo(29, 20))?;
        stdout.queue(PrintStyledContent("OD".cyan()))?;
        stdout.queue(MoveTo(34, 13))?;
        stdout.queue(PrintStyledContent("GR".cyan()))?;
        stdout.queue(MoveTo(34, 14))?;
        stdout.queue(PrintStyledContent("LE".cyan()))?;
        stdout.queue(MoveTo(34, 15))?;
        stdout.queue(PrintStyledContent("LS".cyan()))?;
        stdout.queue(MoveTo(34, 16))?;
        stdout.queue(PrintStyledContent("GE".cyan()))?;
        stdout.queue(MoveTo(34, 17))?;
        stdout.queue(PrintStyledContent("EQ".cyan()))?;
        stdout.queue(MoveTo(34, 18))?;
        stdout.queue(PrintStyledContent("NE".cyan()))?;
        stdout.queue(MoveTo(34, 19))?;
        stdout.queue(PrintStyledContent("US".cyan()))?;
        stdout.queue(MoveTo(34, 20))?;
        stdout.queue(PrintStyledContent("TR".cyan()))?;

        draw_box((39, 11), (26, 3), "".to_string())?;
        stdout.queue(SetBackgroundColor(FIELD_COLOR))?;
        stdout.execute(SetAttribute(Attribute::Bold))?;
        stdout.execute(SetAttribute(Attribute::Underlined))?;
        stdout.queue(MoveTo(41, 12))?;
        stdout.queue(PrintStyledContent("PC".magenta()))?;
        stdout.execute(SetAttribute(Attribute::Reset))?;
        let pc = self.pc % 64;
        let bin = format!("{pc:b}");
        stdout.queue(PrintStyledContent(format!(" {bin:0>0$} ", 6).white()))?;
        stdout.queue(PrintStyledContent("MODE: ".cyan()))?;
        self.draw_mode()?;

        draw_box((39, 13), (26, 9), "".to_string())?;

        draw_box((0, 21), (65, 3), "".to_string())?;

        self.draw_help()?;

        stdout.queue(SetBackgroundColor(BG_COLOR))?;
        stdout.queue(MoveTo(46, 1))?;
        stdout.queue(PrintStyledContent("╦".white()))?;
        stdout.queue(MoveTo(46, 11))?;
        stdout.queue(PrintStyledContent("╩".white()))?;

        stdout.queue(MoveTo(0, 11))?;
        stdout.queue(PrintStyledContent("╠".white()))?;

        stdout.queue(MoveTo(9, 11))?;
        stdout.queue(PrintStyledContent("╦".white()))?;
        stdout.queue(MoveTo(9, 21))?;
        stdout.queue(PrintStyledContent("╩".white()))?;

        stdout.queue(MoveTo(18, 11))?;
        stdout.queue(PrintStyledContent("╦".white()))?;
        stdout.queue(MoveTo(18, 21))?;
        stdout.queue(PrintStyledContent("╩".white()))?;

        stdout.queue(MoveTo(27, 11))?;
        stdout.queue(PrintStyledContent("╦".white()))?;
        stdout.queue(MoveTo(27, 21))?;
        stdout.queue(PrintStyledContent("╩".white()))?;

        stdout.queue(MoveTo(39, 11))?;
        stdout.queue(PrintStyledContent("╦".white()))?;
        stdout.queue(MoveTo(39, 21))?;
        stdout.queue(PrintStyledContent("╩".white()))?;

        stdout.queue(MoveTo(0, 23))?;

        stdout.queue(MoveTo(39, 13))?;
        stdout.queue(PrintStyledContent("╠".white()))?;
        stdout.queue(MoveTo(0, 21))?;
        stdout.queue(PrintStyledContent("╠".white()))?;

        stdout.queue(MoveTo(64, 11))?;
        stdout.queue(PrintStyledContent("╣".white()))?;
        stdout.queue(MoveTo(64, 13))?;
        stdout.queue(PrintStyledContent("╣".white()))?;
        stdout.queue(MoveTo(64, 21))?;
        stdout.queue(PrintStyledContent("╣".white()))?;

        Ok(())
    }

    fn cycle(&mut self) -> Result<()> {
        let temp = self.read_from_rom(self.pc);
        let bin = format!("{temp:b}");
        let instruction = format!("{bin:0>0$}", 16);
        let opcode = &instruction[0..4];

        match opcode {
            "0000" => {
                self.pc += 1;
            }
            "0001" => {
                let dest = u16::from_str_radix(&instruction[4..8], 2).unwrap();
                let src_a = usize::from_str_radix(&instruction[8..12], 2).unwrap();
                let src_b = usize::from_str_radix(&instruction[12..16], 2).unwrap();

                self.write_to_regs(dest % 8, (self.reg[src_a % 8] + self.reg[src_b % 8]) % 256)?;

                self.pc += 1;
            }
            "0010" => {
                let dest = u16::from_str_radix(&instruction[4..8], 2).unwrap();
                let src_a = usize::from_str_radix(&instruction[8..12], 2).unwrap();
                let src_b = usize::from_str_radix(&instruction[12..16], 2).unwrap();

                self.write_to_regs(dest % 8, (self.reg[src_a % 8] - self.reg[src_b % 8]) % 256)?;

                self.pc += 1;
            }
            "0011" => {
                let dest = u16::from_str_radix(&instruction[4..8], 2).unwrap();
                let src_a = usize::from_str_radix(&instruction[8..12], 2).unwrap();
                let src_b = usize::from_str_radix(&instruction[12..16], 2).unwrap();

                self.write_to_regs(dest % 8, (self.reg[src_a % 8] & self.reg[src_b % 8]) % 256)?;

                self.pc += 1;
            }
            "0100" => {
                let dest = u16::from_str_radix(&instruction[4..8], 2).unwrap();
                let src_a = usize::from_str_radix(&instruction[8..12], 2).unwrap();
                let src_b = usize::from_str_radix(&instruction[12..16], 2).unwrap();

                self.write_to_regs(dest % 8,!(self.reg[src_a % 8] | self.reg[src_b % 8]) % 256)?;

                self.pc += 1;
            }
            "0101" => {
                let dest = u16::from_str_radix(&instruction[4..8], 2).unwrap();
                let src_a = usize::from_str_radix(&instruction[8..12], 2).unwrap();
                let src_b = usize::from_str_radix(&instruction[12..16], 2).unwrap();

                self.write_to_regs(dest % 8, (self.reg[src_a % 8] ^ self.reg[src_b % 8]) % 256)?;

                self.pc += 1;
            }
            "0110" => {
                let dest = u16::from_str_radix(&instruction[4..8], 2).unwrap();
                let src_a = usize::from_str_radix(&instruction[8..12], 2).unwrap();

                self.write_to_regs(dest % 8, (self.reg[src_a % 8] / 2) % 256)?;

                self.pc += 1;
            }
            "0111" => {
                let src_a = usize::from_str_radix(&instruction[8..12], 2).unwrap();
                let src_b = usize::from_str_radix(&instruction[12..16], 2).unwrap();

                let a = self.reg[src_a % 8] % 256;
                let b = self.reg[src_b % 8] % 256;

                self.flg[8..16].copy_from_slice(&[
                    a > b,
                    a <= b,
                    a < b,
                    a >= b,
                    a == b,
                    a != b,
                    false,
                    true
                ]);

                self.pc += 1;
            }
            "1000" => {
                let dest = u16::from_str_radix(&instruction[4..8], 2).unwrap();
                let imm = u16::from_str_radix(&instruction[8..16], 2).unwrap();

                self.write_to_regs(dest % 8, imm % 256)?;

                self.pc += 1;
            }
            "1001" => {
                let dest = u16::from_str_radix(&instruction[4..8], 2).unwrap();
                let addr = usize::from_str_radix(&instruction[8..16], 2).unwrap();

                self.write_to_regs(dest % 8, self.ram[addr % 32])?;

                self.pc += 1;
            }
            "1010" => {
                let src = u16::from_str_radix(&instruction[4..8], 2).unwrap();
                let addr = u16::from_str_radix(&instruction[8..16], 2).unwrap();

                self.write_to_ram(addr % 32, self.reg[(src % 8) as usize])?;

                self.pc += 1;
            }
            "1011" => {
                let dest = u16::from_str_radix(&instruction[4..8], 2).unwrap();
                let ptr = usize::from_str_radix(&instruction[8..16], 2).unwrap();

                self.write_to_regs(dest, self.ram[(self.reg[(ptr % 8) as usize] % 32) as usize])?;

                self.pc += 1;
            }
            "1100" => {
                let src = u16::from_str_radix(&instruction[4..8], 2).unwrap();
                let ptr = u16::from_str_radix(&instruction[8..16], 2).unwrap();

                self.write_to_ram(self.reg[(ptr % 8) as usize] % 32, self.reg[(src % 8) as usize])?;

                self.pc += 1;
            }
            "1101" => {
                let cond = u16::from_str_radix(&instruction[4..8], 2).unwrap();
                let addr = u16::from_str_radix(&instruction[8..16], 2).unwrap();

                if self.flg[(cond % 16) as usize] {
                    self.pc = addr % 64;
                } else {
                    self.pc += 1;
                }
            }
            "1110" => {
                let cond = u16::from_str_radix(&instruction[4..8], 2).unwrap();
                let ptr = u16::from_str_radix(&instruction[12..16], 2).unwrap();

                if self.flg[(cond % 16) as usize] {
                    self.pc = self.reg[(ptr % 8) as usize] % 64;
                } else {
                    self.pc += 1;
                }
            }
            "1111" => {
                let addr = u16::from_str_radix(&instruction[4..16], 2).unwrap();

                self.pc = addr % 64
            }
            _ => {
                self.pc += 1;
            }
        }

        Ok(())
    }
}

fn draw_box((x_pos, y_pos): (u16, u16), (x_size, y_size): (u16, u16), title: String) -> Result<()> {
    let mut stdout = stdout();

    for i in 0..x_size {
        for j in 0..y_size {
            stdout.queue(MoveTo(i + x_pos, j + y_pos))?;

            stdout.queue(SetBackgroundColor(BG_COLOR))?;

            if i == 0 && j == 0 { stdout.queue(PrintStyledContent("╔".white()))?; }
            else if i == x_size - 1 && j == 0 { stdout.queue(PrintStyledContent("╗".white()))?; }
            else if i == 0 && j == y_size - 1 { stdout.queue(PrintStyledContent("╚".white()))?; }
            else if i == x_size - 1 && j == y_size - 1 { stdout.queue(PrintStyledContent("╝".white()))?; }

            else if i == 0 || i == x_size - 1 { stdout.queue(PrintStyledContent("║".white()))?; }
            else if j == 0 || j == y_size - 1 { stdout.queue(PrintStyledContent("═".white()))?; }

            else if i != 0 && i != x_size && j != 0 && j != y_size - 1 {
                stdout.queue(SetBackgroundColor(FIELD_COLOR))?;
                stdout.queue(PrintStyledContent(" ".white()))?;
            }
        }
    }
    stdout.queue(MoveTo(x_pos + 1, y_pos))?;
    stdout.queue(PrintStyledContent(title.white()))?;

    Ok(())
}

fn main() -> Result<()> {
    let mut stdout = stdout();
    enable_raw_mode()?;

    let mut delay = Duration::from_millis(1000);

    let mut emulator: EmulatorState = EmulatorState {
        rom: [0; 64],
        ram: [0; 32],
        reg: [0; 8],
        inp: [0; 8],
        out: [0; 8],
        flg: [false; 16],
        pc: 0,

        mode: Setup,
        log_buffer: Default::default(),

        current_rom_read: None,
        current_ram_write: None,
        current_reg_write: None,
    };

    emulator.program_reset();

    emulator.rom[0] = 0b1000000000000000;
    emulator.rom[1] = 0b1000000100000001;
    emulator.rom[2] = 0b0001000000010000;
    emulator.rom[3] = 0b0001000100010000;
    emulator.rom[4] = 0b1111000000000010;

    emulator.draw_layout()?;
    emulator.draw_contents()?;

    loop {
        if terminal::size()? != WINDOW_SIZE {
            stdout.queue(SetSize(WINDOW_SIZE.0, WINDOW_SIZE.1))?;
        }

        if poll(Duration::from_millis(0))? {
            if let Event::Key(key) = read()? {
                match &emulator.mode {
                    Setup => {
                        match (key.code, key.kind) {
                            (KeyCode::Char('l'), KeyEventKind::Press) => {

                            }
                            (KeyCode::Char('c'), KeyEventKind::Press) => {
                                emulator.full_reset()?;
                            }
                            (KeyCode::Char('r'), KeyEventKind::Press) => {
                                emulator.mode = Automatic(100);
                            }
                            (KeyCode::Char('s'), KeyEventKind::Press) => {
                                emulator.mode = ManualStep;
                            }
                            _ => {

                            }
                        }
                    }
                    ManualStep => {
                        match (key.code, key.kind) {
                            (KeyCode::Char('c'), KeyEventKind::Press) => {
                                emulator.program_reset()?;
                            }
                            (KeyCode::Char('s'), KeyEventKind::Press) => {
                                emulator.cycle()?;
                            }
                            _ => {

                            }
                        }
                    }
                    Automatic(v) => {
                        match (key.code, key.kind) {
                            (KeyCode::Char('c'), KeyEventKind::Press) => {
                                emulator.program_reset()?;
                            }
                            (KeyCode::Char('s'), KeyEventKind::Press) => {
                                emulator.mode = ManualStep;
                            }
                            _ => {

                            }
                        }
                    }
                }
                emulator.draw_mode()?;
                emulator.draw_help()?;
                stdout.flush()?;
            }
            else {
                stdout.queue(terminal::Clear(terminal::ClearType::Purge))?;
                stdout.queue(cursor::Hide)?;
                emulator.draw_layout()?;
                emulator.draw_contents()?;
                stdout.flush()?;
            }
            while poll(Duration::from_millis(0))? {
                read()?;
            }
        } else {

        }
    }
}

