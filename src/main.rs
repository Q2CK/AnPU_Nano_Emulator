use std::{time::{Duration, Instant},
          io::{stdout, Write},
          fs,
          path::Path,
          ffi::OsString};

use crossterm::{QueueableCommand,
                terminal::{self, SetSize, enable_raw_mode, disable_raw_mode, Clear, ClearType},
                cursor::{self, MoveTo},
                style::{Stylize, Color, PrintStyledContent, Attribute, Print, SetAttribute, SetBackgroundColor},
                event::{read, poll, Event, KeyCode, KeyEventKind},
                Result};

use crate::Mode::{Automatic, ManualStep, Setup};

const WINDOW_SIZE: (u16, u16) = (65, 24);

const BG_COLOR: Color = Color::Black;
const FIELD_COLOR: Color = Color::Black;

enum Mode {
    Setup,
    ManualStep,
    Automatic(u16),
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

    executed_instructions: usize,

    current_rom_read: Option<u16>,
    current_ram_write: Option<u16>,
    current_reg_write: Option<u16>,
}

#[allow(arithmetic_overflow)]
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
        self.flg[15] = true;
        self.pc = 0;

        self.mode = Setup;
        self.log_buffer = Default::default();

        self.push_log(format!("Ex. instr: {}", self.executed_instructions))?;
        self.executed_instructions = 0;

        self.reset_last_mods()?;
        self.draw_contents()?;

        Ok(())
    }

    fn reset_last_mods(&mut self) -> Result<()> {
        let mut stdout = stdout();

        if let Some(i) = self.current_rom_read {
            let i = i % 64;
            let value = self.rom[i as usize] % 65536;
            let hex = &format!("{value:x}");
            stdout.queue(MoveTo((5 * (i % 8) + 6) as u16, (i / 8 + 3) as u16))?;
            stdout.queue(PrintStyledContent(format!("{hex:0>0$}", 4).white()))?;
        }

        if let Some(i) = self.current_ram_write {
            let i = i % 32;
            let value = self.ram[i as usize] % 256;
            let hex = &format!("{value:x}");
            stdout.queue(MoveTo((3 * (i % 4) + 52) as u16, (i / 4 + 3) as u16))?;
            stdout.queue(PrintStyledContent(format!("{hex:0>0$}", 2).white()))?;
        }

        if let Some(i) = self.current_reg_write {
            let i = i % 8;
            let val = self.reg[i as usize];
            let hex = &format!("{val:x}");
            stdout.queue(MoveTo(6, 13 + i)).unwrap();
            stdout.queue(PrintStyledContent(format!("{hex:0>0$}", 2).white()))?;
        }

        self.current_rom_read = None;
        self.current_ram_write = None;
        self.current_reg_write = None;

        Ok(())
    }

    fn draw_log(&mut self) -> Result<()> {
        let mut stdout = stdout();

        for i in 0..6 {
            stdout.queue(MoveTo(41, 14 + i))?;
            if self.log_buffer[i as usize].len() < 22 {
                stdout.queue(PrintStyledContent(self.log_buffer[i as usize].clone().white()))?;
            } else {
                stdout.queue(PrintStyledContent(self.log_buffer[i as usize].clone()[..22].white()))?;
            }
        }
        stdout.queue(MoveTo(41, 20))?;
        if self.log_buffer[6].len() < 22 {
            stdout.queue(PrintStyledContent(self.log_buffer[6].clone().green()))?;
        } else {
            stdout.queue(PrintStyledContent(self.log_buffer[6].clone()[..22].green()))?;
        }

        Ok(())
    }

    fn draw_flags(&self) -> Result<()> {
        let mut stdout = stdout();

        for idx in 0..16 {
            stdout.queue(MoveTo(match idx {
                0..=7 => 0,
                _ => 5
            } + 32, idx % 8 + 13))?;
            let value = match self.flg[idx as usize] {
                true => "T",
                false => "F"
            };
            stdout.queue(PrintStyledContent(value.white()))?;
        }

        Ok(())
    }

    fn push_log(&mut self, new_entry: String) -> Result<()> {
        for i in 1..7 {
            self.log_buffer[i - 1] = self.log_buffer[i].clone();
        }
        self.log_buffer[6] = format!("{: <22}", new_entry);

        self.draw_log()?;
        Ok(())
    }

    fn draw_contents(&mut self) -> Result<()> {
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

        self.draw_pc()?;
        self.draw_flags()?;

        self.draw_log()?;

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

    fn draw_pc(&mut self) -> Result<()> {
        let mut stdout = stdout();

        stdout.queue(SetBackgroundColor(FIELD_COLOR))?;
        stdout.queue(SetAttribute(Attribute::Bold))?;
        stdout.queue(SetAttribute(Attribute::Underlined))?;
        stdout.queue(MoveTo(41, 12))?;
        stdout.queue(PrintStyledContent("PC".magenta()))?;
        stdout.queue(SetAttribute(Attribute::Reset))?;
        let pc = self.pc % 64;
        let bin = format!("{pc:b}");
        stdout.queue(PrintStyledContent(format!(" {bin:0>0$} ", 6).white()))?;
        stdout.queue(PrintStyledContent("MODE: ".cyan()))?;
        self.draw_mode()?;

        Ok(())
    }

    fn draw_mode(&mut self) -> Result<()> {
        let mut stdout = stdout();

        stdout.queue(MoveTo(57, 12))?;
        match self.mode {
            Setup => stdout.queue(PrintStyledContent("HALTED".red()))?,
            ManualStep => stdout.queue(PrintStyledContent("MANUAL".yellow()))?,
            Automatic(_) => stdout.queue(PrintStyledContent("SWOOSH".green()))?// stdout.queue(PrintStyledContent(format!("{speed:->0$}", 6).green()))?
        };

        Ok(())
    }

    fn draw_help(&mut self) -> Result<()> {
        let mut stdout = stdout();

        match self.mode {
            Setup => {
                stdout.queue(MoveTo(2, 22))?;
                stdout.queue(PrintStyledContent("L".cyan()))?;
                stdout.queue(PrintStyledContent(" - Load next program ".white()))?;
                stdout.queue(PrintStyledContent("C".cyan()))?;
                stdout.queue(PrintStyledContent(" - clear ".white()))?;
                stdout.queue(PrintStyledContent("R".cyan()))?;
                stdout.queue(PrintStyledContent(" - run ".white()))?;
                stdout.queue(PrintStyledContent("S".cyan()))?;
                stdout.queue(PrintStyledContent(" - step ".white()))?;
                stdout.queue(PrintStyledContent("Q".cyan()))?;
                stdout.queue(PrintStyledContent(" - quit     ".white()))?;
            }
            ManualStep | Automatic(_) => {
                stdout.queue(MoveTo(2, 22))?;
                stdout.queue(PrintStyledContent(" ".cyan()))?;
                stdout.queue(PrintStyledContent("                     ".white()))?;
                stdout.queue(PrintStyledContent("C".cyan()))?;
                stdout.queue(PrintStyledContent(" - clear ".white()))?;
                stdout.queue(PrintStyledContent(" ".cyan()))?;
                stdout.queue(PrintStyledContent("       ".white()))?;
                stdout.queue(PrintStyledContent("S".cyan()))?;
                stdout.queue(PrintStyledContent(" - step ".white()))?;
                stdout.queue(PrintStyledContent("             ".white()))?;
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
            stdout.queue(MoveTo(3 * (i % 4) + 52, (i / 4 + 3) as u16)).unwrap();
            stdout.queue(PrintStyledContent(format!("{hex:0>0$}", 2).white())).unwrap();
        }

        if let Some(i) = self.current_reg_write {
            let i = i % 8;
            let val = self.reg[i as usize];
            stdout.queue(MoveTo(6, 13 + i)).unwrap();
            let hex = &format!("{val:x}");
            stdout.queue(PrintStyledContent(format!("{hex:0>0$}", 2).white())).unwrap();
        }

        stdout.queue(MoveTo(0, 0))?;
        stdout.queue(SetBackgroundColor(Color::Magenta))?;
        stdout.queue(SetAttribute(Attribute::Bold))?;
        stdout.queue(SetAttribute(Attribute::Underlined))?;
        stdout.queue(PrintStyledContent(" AnPU Nano emulator                                              ".white()))?;
        stdout.queue(SetAttribute(Attribute::Reset))?;


        draw_box((0, 1), (47, 11), "".to_string())?;
        stdout.queue(SetBackgroundColor(FIELD_COLOR))?;
        stdout.queue(SetAttribute(Attribute::Bold))?;
        stdout.queue(SetAttribute(Attribute::Underlined))?;
        stdout.queue(MoveTo(2, 2))?;
        stdout.queue(PrintStyledContent("ROM".magenta()))?;
        stdout.queue(SetAttribute(Attribute::Reset))?;
        stdout.queue(SetBackgroundColor(FIELD_COLOR))?;
        stdout.queue(MoveTo(6, 2))?;
        stdout.queue(PrintStyledContent(" 000  001  010  011  100  101  110  111".cyan()))?;
        for i in 0..8 {
            stdout.queue(MoveTo(2, 3 + i))?;

            let bin = format!("{i:b}");
            stdout.queue(PrintStyledContent(format!("{bin:0>0$}", 3).cyan()))?;
        }

        draw_box((46, 1), (19, 11), "".to_string())?;
        stdout.queue(SetBackgroundColor(FIELD_COLOR))?;
        stdout.queue(SetAttribute(Attribute::Bold))?;
        stdout.queue(SetAttribute(Attribute::Underlined))?;
        stdout.queue(MoveTo(48, 2))?;
        stdout.queue(PrintStyledContent("RAM".magenta()))?;
        stdout.queue(SetAttribute(Attribute::Reset))?;
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
        stdout.queue(SetAttribute(Attribute::Bold))?;
        stdout.queue(SetAttribute(Attribute::Underlined))?;
        stdout.queue(MoveTo(2, 12))?;
        stdout.queue(PrintStyledContent("REG".magenta()))?;
        stdout.queue(SetAttribute(Attribute::Reset))?;
        stdout.queue(SetBackgroundColor(FIELD_COLOR))?;
        for i in 0..8 {
            stdout.queue(MoveTo(2, 13 + i))?;
            let bin = format!("{i:b}");
            stdout.queue(PrintStyledContent(format!("{bin:0>0$} ", 3).cyan()))?;
        }

        draw_box((9, 11), (10, 11), "".to_string())?;
        stdout.queue(SetBackgroundColor(FIELD_COLOR))?;
        stdout.queue(SetAttribute(Attribute::Bold))?;
        stdout.queue(SetAttribute(Attribute::Underlined))?;
        stdout.queue(MoveTo(11, 12))?;
        stdout.queue(PrintStyledContent("INP".magenta()))?;
        stdout.queue(SetAttribute(Attribute::Reset))?;
        stdout.queue(SetBackgroundColor(FIELD_COLOR))?;
        for i in 0..8 {
            let bin = format!("{i:b}");
            stdout.queue(MoveTo(11, 13 + i))?;
            stdout.queue(PrintStyledContent(format!("{bin:0>0$} ", 3).cyan()))?;
        }

        draw_box((18, 11), (10, 11), "".to_string())?;
        stdout.queue(SetBackgroundColor(FIELD_COLOR))?;
        stdout.queue(SetAttribute(Attribute::Bold))?;
        stdout.queue(SetAttribute(Attribute::Underlined))?;
        stdout.queue(MoveTo(20, 12))?;
        stdout.queue(PrintStyledContent("OUT".magenta()))?;
        stdout.queue(SetAttribute(Attribute::Reset))?;
        stdout.queue(SetBackgroundColor(FIELD_COLOR))?;
        for i in 0..8 {
            let bin = format!("{i:b}");
            stdout.queue(MoveTo(20, 13 + i))?;
            stdout.queue(PrintStyledContent(format!("{bin:0>0$} ", 3).cyan()))?;
        }

        draw_box((27, 11), (13, 11), "".to_string())?;
        stdout.queue(SetBackgroundColor(FIELD_COLOR))?;
        stdout.queue(SetAttribute(Attribute::Bold))?;
        stdout.queue(SetAttribute(Attribute::Underlined))?;
        stdout.queue(MoveTo(29, 12))?;
        stdout.queue(PrintStyledContent("FLG".magenta()))?;
        stdout.queue(SetAttribute(Attribute::Reset))?;
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

    /*fn alu_flags(&mut self, result: u16) {

    }*/

    fn cycle(&mut self) -> Result<()> {
        self.draw_pc()?;
        let temp = self.read_from_rom(self.pc);
        let bin = format!("{temp:b}");
        let instruction = format!("{bin:0>0$}", 16);
        let opcode = &instruction[0..4];

        self.executed_instructions += 1;

        match opcode {
            "0000" => {
                self.mode = Setup;
                self.draw_mode()?;
                self.pc += 1;
                self.push_log("int".to_string())?;
            }
            "0001" => {
                let dest = u16::from_str_radix(&instruction[4..8], 2).unwrap();
                let src_a = usize::from_str_radix(&instruction[8..12], 2).unwrap();
                let src_b = usize::from_str_radix(&instruction[12..16], 2).unwrap();

                self.flg[0] = (self.reg[src_a % 8] + self.reg[src_b % 8]) % 256 == 0;
                self.flg[1] = (self.reg[src_a % 8] + self.reg[src_b % 8]) % 256 != 0;
                self.flg[2] = (self.reg[src_a % 8] % 256) + (self.reg[src_b % 8] % 256) & 0x0100 != 0;
                self.flg[3] = (self.reg[src_a % 8] % 256) + (self.reg[src_b % 8] % 256) & 0x0100 == 0;
                self.flg[4] = ((self.reg[src_a % 8] % 128) + (self.reg[src_b % 8] % 128) & 0x0080 != 0)
                            ^ self.flg[2];
                self.flg[5] = !self.flg[4];
                self.flg[6] = (self.reg[src_a % 8] + self.reg[src_b % 8]) % 2 == 0;
                self.flg[7] = (self.reg[src_a % 8] + self.reg[src_b % 8]) % 2 != 0;

                self.write_to_regs(dest % 8, (self.reg[src_a % 8] + self.reg[src_b % 8]) % 256)?;

                self.pc += 1;

                self.push_log(format!("add {}, {}, {}", dest % 8, src_a % 8, src_b % 8))?;
            }
            "0010" => {
                let dest = u16::from_str_radix(&instruction[4..8], 2).unwrap();
                let src_a = usize::from_str_radix(&instruction[8..12], 2).unwrap();
                let src_b = usize::from_str_radix(&instruction[12..16], 2).unwrap();

                self.flg[0] = (self.reg[src_a % 8] - self.reg[src_b % 8]) % 256 == 0;
                self.flg[1] = (self.reg[src_a % 8] - self.reg[src_b % 8]) % 256 != 0;
                self.flg[2] = (self.reg[src_a % 8] % 256) - (self.reg[src_b % 8] % 256) & 0x0100 != 0;
                self.flg[3] = (self.reg[src_a % 8] % 256) - (self.reg[src_b % 8] % 256) & 0x0100 == 0;
                self.flg[4] = ((self.reg[src_a % 8] % 128) - (self.reg[src_b % 8] % 128) & 0x0080 != 0)
                            ^ self.flg[2];
                self.flg[5] = !self.flg[4];
                self.flg[6] = (self.reg[src_a % 8] - self.reg[src_b % 8]) % 2 == 0;
                self.flg[7] = (self.reg[src_a % 8] - self.reg[src_b % 8]) % 2 != 0;

                self.write_to_regs(dest % 8, (self.reg[src_a % 8] - self.reg[src_b % 8]) % 256)?;

                self.pc += 1;

                self.push_log(format!("sub {}, {}, {}", dest % 8, src_a % 8, src_b % 8))?;
            }
            "0011" => {
                let dest = u16::from_str_radix(&instruction[4..8], 2).unwrap();
                let src_a = usize::from_str_radix(&instruction[8..12], 2).unwrap();
                let src_b = usize::from_str_radix(&instruction[12..16], 2).unwrap();

                self.flg[0] = (self.reg[src_a % 8] & self.reg[src_b % 8]) % 256 == 0;
                self.flg[1] = (self.reg[src_a % 8] & self.reg[src_b % 8]) % 256 != 0;
                self.flg[2] = false;
                self.flg[3] = false;
                self.flg[4] = false;
                self.flg[5] = false;
                self.flg[6] = (self.reg[src_a % 8] & self.reg[src_b % 8]) % 2 == 0;
                self.flg[7] = (self.reg[src_a % 8] & self.reg[src_b % 8]) % 2 != 0;

                self.write_to_regs(dest % 8, (self.reg[src_a % 8] & self.reg[src_b % 8]) % 256)?;

                self.pc += 1;

                self.push_log(format!("and {}, {}, {}", dest % 8, src_a % 8, src_b % 8))?;
            }
            "0100" => {
                let dest = u16::from_str_radix(&instruction[4..8], 2).unwrap();
                let src_a = usize::from_str_radix(&instruction[8..12], 2).unwrap();
                let src_b = usize::from_str_radix(&instruction[12..16], 2).unwrap();

                self.flg[0] = !(self.reg[src_a % 8] | self.reg[src_b % 8]) % 256 == 0;
                self.flg[1] = !(self.reg[src_a % 8] | self.reg[src_b % 8]) % 256 != 0;
                self.flg[2] = false;
                self.flg[3] = false;
                self.flg[4] = false;
                self.flg[5] = false;
                self.flg[6] = !(self.reg[src_a % 8] | self.reg[src_b % 8]) % 2 == 0;
                self.flg[7] = !(self.reg[src_a % 8] | self.reg[src_b % 8]) % 2 != 0;

                self.write_to_regs(dest % 8, !(self.reg[src_a % 8] | self.reg[src_b % 8]) % 256)?;

                self.pc += 1;

                self.push_log(format!("nor {}, {}, {}", dest % 8, src_a % 8, src_b % 8))?;
            }
            "0101" => {
                let dest = u16::from_str_radix(&instruction[4..8], 2).unwrap();
                let src_a = usize::from_str_radix(&instruction[8..12], 2).unwrap();
                let src_b = usize::from_str_radix(&instruction[12..16], 2).unwrap();

                self.flg[0] = (self.reg[src_a % 8] ^ self.reg[src_b % 8]) % 256 == 0;
                self.flg[1] = (self.reg[src_a % 8] ^ self.reg[src_b % 8]) % 256 != 0;
                self.flg[2] = false;
                self.flg[3] = false;
                self.flg[4] = false;
                self.flg[5] = false;
                self.flg[6] = (self.reg[src_a % 8] ^ self.reg[src_b % 8]) % 2 == 0;
                self.flg[7] = (self.reg[src_a % 8] ^ self.reg[src_b % 8]) % 2 != 0;

                self.write_to_regs(dest % 8, (self.reg[src_a % 8] ^ self.reg[src_b % 8]) % 256)?;

                self.pc += 1;

                self.push_log(format!("xor {}, {}, {}", dest % 8, src_a % 8, src_b % 8))?;
            }
            "0110" => {
                let dest = u16::from_str_radix(&instruction[4..8], 2).unwrap();
                let src_a = usize::from_str_radix(&instruction[8..12], 2).unwrap();

                self.flg[0] = (self.reg[src_a % 8] >>1) % 256 == 0;
                self.flg[1] = (self.reg[src_a % 8] >> 1) % 256 != 0;
                self.flg[2] = false;
                self.flg[3] = false;
                self.flg[4] = false;
                self.flg[5] = false;
                self.flg[6] = (self.reg[src_a % 8] >> 1) % 2 == 0;
                self.flg[7] = (self.reg[src_a % 8] >> 1) % 2 != 0;

                self.write_to_regs(dest % 8, (self.reg[src_a % 8] >> 1) % 256)?;

                self.pc += 1;

                self.push_log(format!("rsh {}, {}", dest % 8, src_a % 8))?;
            }
            "0111" => {
                let src_a = usize::from_str_radix(&instruction[8..12], 2).unwrap();
                let src_b = usize::from_str_radix(&instruction[12..16], 2).unwrap();

                let a = self.reg[src_a % 8] % 256;
                let b = self.reg[src_b % 8] % 256;

                self.flg[8] = a > b;
                self.flg[9] = a <= b;
                self.flg[10] = a < b;
                self.flg[11] = a >= b;
                self.flg[12] = a == b;
                self.flg[13] = a != b;
                self.flg[14] = false;
                self.flg[15] = true;

                self.pc += 1;

                self.push_log(format!("cmp {}, {}", src_a % 8, src_b % 8))?;
            }
            "1000" => {
                let dest = u16::from_str_radix(&instruction[4..8], 2).unwrap();
                let imm = u16::from_str_radix(&instruction[8..16], 2).unwrap();

                self.write_to_regs(dest % 8, imm % 256)?;

                self.pc += 1;

                self.push_log(format!("imm {}, {}", dest % 8, imm % 256))?;
            }
            "1001" => {
                let dest = u16::from_str_radix(&instruction[4..8], 2).unwrap();
                let addr = usize::from_str_radix(&instruction[8..16], 2).unwrap();

                self.write_to_regs(dest % 8, self.ram[addr % 32])?;

                self.pc += 1;

                self.push_log(format!("dml {}, {}", dest % 8, addr % 32))?;
            }
            "1010" => {
                let src = u16::from_str_radix(&instruction[4..8], 2).unwrap();
                let addr = u16::from_str_radix(&instruction[8..16], 2).unwrap();

                self.write_to_ram(addr % 32, self.reg[(src % 8) as usize])?;

                self.pc += 1;

                self.push_log(format!("dms {}, {}", src % 8, addr % 32))?;
            }
            "1011" => {
                let dest = u16::from_str_radix(&instruction[4..8], 2).unwrap();
                let ptr = usize::from_str_radix(&instruction[8..12], 2).unwrap();

                self.write_to_regs(dest, self.ram[(self.reg[(ptr % 8)] % 32) as usize])?;

                self.pc += 1;

                self.push_log(format!("iml {}, {}", dest % 8, ptr % 8))?;
            }
            "1100" => {
                let src = u16::from_str_radix(&instruction[12..16], 2).unwrap();
                let ptr = u16::from_str_radix(&instruction[8..12], 2).unwrap();

                self.write_to_ram(self.reg[(ptr % 8) as usize] % 32, self.reg[(src % 8) as usize])?;

                self.pc += 1;

                self.push_log(format!("ims {}, {}", ptr % 8, src % 8))?;
            }
            "1101" => {
                let cond = u16::from_str_radix(&instruction[4..8], 2).unwrap();
                let addr = u16::from_str_radix(&instruction[8..16], 2).unwrap();

                if self.flg[(cond % 16) as usize] {
                    self.pc = addr % 64;
                } else {
                    self.pc += 1;
                }

                self.push_log(format!("brc {}, {}", cond % 16, addr % 64))?;
            }
            "1110" => {
                let cond = u16::from_str_radix(&instruction[4..8], 2).unwrap();
                let ptr = u16::from_str_radix(&instruction[12..16], 2).unwrap();

                if self.flg[(cond % 16) as usize] {
                    self.pc = self.reg[(ptr % 8) as usize] % 64;
                } else {
                    self.pc += 1;
                }

                self.push_log(format!("ibr {}, 0, {}", cond % 16, ptr % 8))?;
            }
            "1111" => {
                let addr = u16::from_str_radix(&instruction[4..16], 2).unwrap();

                self.pc = addr % 64;

                self.push_log(format!("jmp {}", addr % 64))?;
            }
            _ => {
                self.pc += 1;

                self.push_log("unknown opcode".to_string())?;
            }
        }

        self.draw_flags()?;

        Ok(())
    }

    fn load_from_file(&mut self, rom_file_name: &str) -> Result<()> {
        match fs::read_to_string(Path::new(rom_file_name)) {
            Ok(v) => {
                let lines: Vec<String> = v.split('\n').map(|x| x.trim().to_string()).collect();
                for (idx, line) in lines.iter().enumerate() {
                    if line.len() == 16 {
                        match u32::from_str_radix(line, 2) {
                            Ok(p) => {
                                self.write_to_rom(idx as u16, p)?;
                            }
                            Err(_) => {
                                self.push_log("Rom init. corrupted".to_string())?;
                                return Ok(());
                            }
                        }
                    }
                }
                self.reset_last_mods()?;
                self.push_log(format!("Loaded {}", rom_file_name))?;
            }
            Err(_) => {
                self.push_log("Program not found".to_string())?;
            }
        }
        match fs::read_to_string(Path::new("ram.bin")) {
            Ok(v) => {
                let lines: Vec<String> = v.split('\n').map(|x| x.trim().to_string()).collect();
                for (idx, line) in lines.iter().enumerate() {
                    if line.len() == 8 {
                        match u16::from_str_radix(line, 2) {
                            Ok(p) => {
                                self.write_to_ram(idx as u16, p)?;
                            }
                            Err(_) => {
                                self.push_log("Ram init. corrupted".to_string())?;
                                return Ok(());
                            }
                        }
                    }
                }
                self.reset_last_mods()?;
                self.push_log("Loaded RAM preset".to_string())?;
            }
            Err(_) => {}
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

            if i == 0 && j == 0 { stdout.queue(PrintStyledContent("╔".white()))?; } else if i == x_size - 1 && j == 0 { stdout.queue(PrintStyledContent("╗".white()))?; } else if i == 0 && j == y_size - 1 { stdout.queue(PrintStyledContent("╚".white()))?; } else if i == x_size - 1 && j == y_size - 1 { stdout.queue(PrintStyledContent("╝".white()))?; } else if i == 0 || i == x_size - 1 { stdout.queue(PrintStyledContent("║".white()))?; } else if j == 0 || j == y_size - 1 { stdout.queue(PrintStyledContent("═".white()))?; } else if i != 0 && i != x_size && j != 0 && j != y_size - 1 {
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
    let size_restore: (u16, u16) = terminal::size()?;

    let mut stdout = stdout();
    enable_raw_mode()?;

    let mut emulator: EmulatorState = EmulatorState {
        rom: [0; 64],
        ram: [0; 32],
        reg: [0; 8],
        inp: [0; 8],
        out: [0; 8],
        flg: [false, false, false, false, false, false, false, false,
            false, false, false, false, false, false, false, true],
        pc: 0,

        mode: Setup,
        log_buffer: Default::default(),

        executed_instructions: 0,

        current_rom_read: None,
        current_ram_write: None,
        current_reg_write: None,
    };

    emulator.program_reset()?;

    emulator.draw_layout()?;
    emulator.draw_contents()?;

    let mut path_idx = 0;
    let mut now = Instant::now();

    let mut delay: u128 = 0;

    loop {
        if terminal::size()? != WINDOW_SIZE {
            stdout.queue(SetSize(WINDOW_SIZE.0, WINDOW_SIZE.1))?;
        }

        if poll(Duration::from_micros(0))? {
            if let Event::Key(key) = read()? {
                match &emulator.mode {
                    Setup => {
                        match (key.code, key.kind) {
                            (KeyCode::Char('l'), KeyEventKind::Press) => {
                                emulator.full_reset()?;

                                let paths: Vec<OsString> = fs::read_dir("./")
                                    .unwrap()
                                    .map(|x| x.unwrap().file_name())
                                    .filter(|x|
                                        x.to_str().unwrap().ends_with(".bin")
                                            && x.to_str().unwrap() != "ram.bin"
                                    )
                                    .collect();
                                if path_idx < paths.len() {
                                    emulator.load_from_file(paths[path_idx].to_str().unwrap())?;
                                }
                                path_idx += 1;
                                if path_idx >= paths.len() {
                                    path_idx = 0;
                                }
                            }
                            (KeyCode::Char('c'), KeyEventKind::Press) => {
                                emulator.full_reset()?;
                            }
                            (KeyCode::Char('r'), KeyEventKind::Press) => {
                                emulator.mode = Automatic(0);
                            }
                            (KeyCode::Char('s'), KeyEventKind::Press) => {
                                emulator.mode = ManualStep;
                            }
                            (KeyCode::Char('q'), KeyEventKind::Press) => {
                                disable_raw_mode()?;
                                stdout.queue(SetSize(size_restore.0, size_restore.1))?;
                                stdout.queue(MoveTo(0,0))?;
                                stdout.queue(Clear(ClearType::Purge))?;
                                stdout.queue(Clear(ClearType::All))?;
                                return Ok(())
                            }
                            _ => {}
                        }
                    }
                    ManualStep => {
                        match (key.code, key.kind) {
                            (KeyCode::Char('c'), KeyEventKind::Press) => {
                                emulator.program_reset()?;
                            }
                            (KeyCode::Char('s'), KeyEventKind::Press) => {
                                emulator.cycle()?;
                                let elapsed_time = now.elapsed().as_micros();
                                now = Instant::now();
                                let frequency: f64 = 1000000f64 / elapsed_time as f64;
                                let freq_string = format!("{:.2}", frequency);
                                stdout.queue(MoveTo(51, 0))?;
                                stdout.queue(SetBackgroundColor(Color::Magenta))?;
                                stdout.queue(SetAttribute(Attribute::Bold))?;
                                stdout.queue(SetAttribute(Attribute::Underlined))?;
                                stdout.queue(PrintStyledContent(format!("{: >10} Hz", freq_string).white()))?;
                                stdout.queue(SetBackgroundColor(BG_COLOR))?;
                                stdout.queue(SetAttribute(Attribute::Reset))?;
                            }
                            _ => {}
                        }
                    }
                    Automatic(_) => {
                        match (key.code, key.kind) {
                            (KeyCode::Char('c'), KeyEventKind::Press) => {
                                emulator.program_reset()?;
                            }
                            (KeyCode::Char('s'), KeyEventKind::Press) => {
                                emulator.mode = ManualStep;
                            }
                            _ => {}
                        }
                    }
                }
                emulator.draw_help()?;
                stdout.flush()?;
            } else {
                stdout.queue(terminal::Clear(terminal::ClearType::Purge))?;
                stdout.queue(cursor::Hide)?;
                emulator.draw_layout()?;
                emulator.draw_contents()?;
                stdout.flush()?;
            }
            while poll(Duration::from_millis(0))? {
                read()?;
            }
        }
        if let Automatic(_) = emulator.mode {
            emulator.cycle()?;
            delay += 1;
            let elapsed_time = now.elapsed().as_micros();
            now = Instant::now();
            if delay % 100 == 0 {
                let frequency: f64 = 1000000f64 / elapsed_time as f64;
                let freq_string = format!("{:.2}", frequency);
                stdout.queue(MoveTo(51, 0))?;
                stdout.queue(SetBackgroundColor(Color::Magenta))?;
                stdout.queue(SetAttribute(Attribute::Bold))?;
                stdout.queue(SetAttribute(Attribute::Underlined))?;
                stdout.queue(PrintStyledContent(format!("{: >10} Hz", freq_string).white()))?;
                stdout.queue(SetBackgroundColor(BG_COLOR))?;
                stdout.queue(SetAttribute(Attribute::Reset))?;
            }
            stdout.flush()?;
        }
    }
}
