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
use crossterm::style::Attributes;
use crate::Mode::Setup;

const WINDOW_SIZE: (u16, u16) = (65, 24);

const BG_COLOR: Color = Color::Black;
const FIELD_COLOR: Color = Color::Black;

enum Mode {
    Setup,
    ManualStep,
    Automatic(u16)
}

struct CpuState {
    rom: [u32; 64],
    ram: [u16; 32],
    reg: [u16; 8],
    inp: [u16; 8],
    out: [u16; 8],
    flg: [bool; 16],
    pc: u16
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

fn draw_layout(cpu_state: &CpuState, mode: &Mode) -> Result<()> {
    let mut stdout = stdout();

    stdout.queue(SetBackgroundColor(BG_COLOR))?;

    for i in 0..WINDOW_SIZE.0 {
        for j in 0..WINDOW_SIZE.1 {
            stdout.queue(MoveTo(i, j))?;
            stdout.queue(Print(" "))?;
        }
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
    for (idx, cell) in cpu_state.rom.iter().enumerate() {
        let value = cell % 65536;
        let hex = &format!("{value:x}");
        stdout.queue(MoveTo((5 * (idx % 8) + 6) as u16, (idx / 8 + 3) as u16))?;
        stdout.queue(PrintStyledContent(format!("{hex:0>0$}", 4).white()))?;
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
    for (idx, cell) in cpu_state.ram.iter().enumerate() {
        let value = cell % 256;
        let hex = &format!("{value:x}");
        stdout.queue(MoveTo((3 * (idx % 4) + 52) as u16, (idx / 4 + 3) as u16))?;
        stdout.queue(PrintStyledContent(format!("{hex:0>0$}", 2).white()))?;
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
        let value = cpu_state.reg[i as usize];
        let hex = &format!("{value:x}");
        stdout.queue(PrintStyledContent(format!("{hex:0>0$}", 2).white()))?;
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
        stdout.queue(MoveTo(11, 13 + i))?;

        let bin = format!("{i:b}");
        stdout.queue(PrintStyledContent(format!("{bin:0>0$} ", 3).cyan()))?;
        let value = cpu_state.inp[i as usize];
        let hex = &format!("{value:x}");
        stdout.queue(PrintStyledContent(format!("{hex:0>0$}", 2).white()))?;
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
        stdout.queue(MoveTo(20, 13 + i))?;

        let bin = format!("{i:b}");
        stdout.queue(PrintStyledContent(format!("{bin:0>0$} ", 3).cyan()))?;
        let value = cpu_state.out[i as usize];
        let hex = &format!("{value:x}");
        stdout.queue(PrintStyledContent(format!("{hex:0>0$}", 2).white()))?;
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
    for i in 0..16 {
        stdout.queue(MoveTo((i / 8) as u16 * 5 + 32, i % 8 + 13))?;

        let value = match cpu_state.flg[i as usize] {
            true => 'T',
            false => 'F'
        };
        stdout.queue(PrintStyledContent(value.white()))?;
    }

    draw_box((39, 11), (26, 3), "".to_string())?;
    stdout.queue(SetBackgroundColor(FIELD_COLOR))?;
    stdout.execute(SetAttribute(Attribute::Bold))?;
    stdout.execute(SetAttribute(Attribute::Underlined))?;
    stdout.queue(MoveTo(41, 12))?;
    stdout.queue(PrintStyledContent("PC".magenta()))?;
    stdout.execute(SetAttribute(Attribute::Reset))?;
    let pc = cpu_state.pc % 64;
    let bin = format!("{pc:b}");
    stdout.queue(PrintStyledContent(format!(" {bin:0>0$} ", 6).white()))?;
    stdout.queue(PrintStyledContent("MODE: ".cyan()))?;
    match mode {
        Setup => stdout.queue(PrintStyledContent("HALTED".red()))?,
        Mode::ManualStep => stdout.queue(PrintStyledContent("MANUAL".yellow()))?,
        Mode::Automatic(speed) => stdout.queue(PrintStyledContent(format!("{}", speed).green()))?
    };

    draw_box((39, 13), (26, 9), "".to_string())?;

    draw_box((0, 21), (65, 3), "".to_string())?;

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

    stdout.execute(MoveTo(0, 23))?;

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

fn main() -> Result<()> {
    let mut stdout = stdout();
    enable_raw_mode()?;

    let delay = Duration::from_millis(100);

    let mut mode: Mode = Setup;
    let mut cpu_state: CpuState = CpuState {
        rom: [0; 64],
        ram: [0; 32],
        reg: [0; 8],
        inp: [0; 8],
        out: [0; 8],
        flg: [false; 16],
        pc: 0
    };

    loop {
        if terminal::size()? != WINDOW_SIZE {
            stdout.queue(SetSize(WINDOW_SIZE.0, WINDOW_SIZE.1))?;
        }
        if poll(Duration::from_millis(0))? {
            if let Event::Key(key) = read()? {
                match &mode {
                    Setup => {

                    }
                    ManualStep => {

                    }
                    Automatic => {

                    }
                }
            }
            else {
                stdout.queue(terminal::Clear(terminal::ClearType::Purge))?;
                stdout.queue(cursor::Hide)?;
                draw_layout(&cpu_state, &mode)?;
                stdout.flush()?;
            }
            while poll(Duration::from_millis(1))? {
                read()?;
            }
        } else {
            // Timeout expired, no `Event` is available
        }
    }
}

