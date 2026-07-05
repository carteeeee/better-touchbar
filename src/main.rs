use anyhow::Result;

use drm::control::ClipRect;

use input::{
    event::{
        //device::DeviceEvent,
        keyboard::{KeyState, KeyboardEvent, KeyboardEventTrait},
        touch::{TouchEvent, TouchEventPosition},
        Event,
    },
    Libinput, LibinputInterface,
};

use input_linux::{uinput::UInputHandle, EventKind, Key, SynchronizeKind};
use input_linux_sys::{input_event, input_id, timeval, uinput_setup};

use noise::{NoiseFn, Perlin};

use palette::{FromColor, Okhsva, Srgba};

use rand::prelude::*;

use std::cmp::PartialOrd;
use std::{
    fs::{File, OpenOptions},
    os::{
        fd::AsRawFd,
        unix::{fs::OpenOptionsExt, io::OwnedFd},
    },
   path::Path,
};

use std::f64::consts::{PI, TAU};

use libc::{c_char, O_ACCMODE, O_RDONLY, O_RDWR, O_WRONLY};

mod display;

const F: usize = 2;
const S: f64 = 0.0012 * PI;
const T: f64 = 0.015;
const R: f64 = 0.5;
const NUM: usize = (2048 * 64) / F / F;

fn clamp<T: PartialOrd>(x: T, min: T, max: T) -> T {
    if x < min {
        min
    } else if x > max {
        max
    } else {
        x
    }
}

struct Interface;

impl LibinputInterface for Interface {
    fn open_restricted(&mut self, path: &Path, flags: i32) -> Result<OwnedFd, i32> {
        let mode = flags & O_ACCMODE;

        OpenOptions::new()
            .custom_flags(flags)
            .read(mode == O_RDONLY || mode == O_RDWR)
            .write(mode == O_WRONLY || mode == O_RDWR)
            .open(path)
            .map(|file| file.into())
            .map_err(|err| err.raw_os_error().unwrap())
    }
    fn close_restricted(&mut self, fd: OwnedFd) {
        _ = File::from(fd);
    }
}

fn emit<F>(uinput: &mut UInputHandle<F>, ty: EventKind, code: u16, value: i32)
where
    F: AsRawFd,
{
    uinput
        .write(&[input_event {
            value,
            type_: ty as u16,
            code,
            time: timeval {
                tv_sec: 0,
                tv_usec: 0,
            },
        }])
        .unwrap();
}

fn toggle_keys<F>(uinput: &mut UInputHandle<F>, codes: &Vec<Key>, value: i32)
where
    F: AsRawFd,
{
    if codes.is_empty() {
        return;
    }
    for kc in codes {
        emit(uinput, EventKind::Key, *kc as u16, value);
    }
    emit(
        uinput,
        EventKind::Synchronize,
        SynchronizeKind::Report as u16,
        0,
    );
}

fn press_keys<F>(uinput: &mut UInputHandle<F>, codes: &Vec<Key>)
where
    F: AsRawFd,
{
    toggle_keys(uinput, codes, true as i32);
    toggle_keys(uinput, codes, false as i32);
}

#[derive(Clone, Debug)]
struct Field {
    t: f64,
    x: f64,
    y: f64,
    vx: f64,
    vy: f64,
}

impl Field {
    /// how much to move from this field at (x, y)
    pub fn to_move(&self, x: f64, y: f64) -> (f64, f64) {
        let d = (16.0 / (self.x - x).hypot(self.y - y)) * (1.0 / (self.t + 1.0));
        (self.vx * d, self.vy * d)
    }

    pub fn update(&self) -> Self {
        let mut copy = self.clone();
        copy.t += 1.0;
        copy
    }
}

fn dot(buffer: &mut [[u8; 4]], width: u32, height: u32, x: u32, y: u32, color: [u8; 4]) {
    buffer[
        (clamp(x, 0, width - 1) +
         clamp(y, 0, height - 1) *
         width) as usize]
            .copy_from_slice(&color);
}

#[allow(unreachable_code)]
fn main() -> Result<()> {
    // gfx
    let mut drm = display::DrmBackend::open_card()?;
    let (real_width, real_height) = drm.mode().size();
    let (width, height) = drm.fb_info()?.size();

    // input
    let mut input_tb = Libinput::new_with_udev(Interface);
    let mut input_main = Libinput::new_with_udev(Interface);
    input_tb.udev_assign_seat("seat-touchbar").unwrap();
    input_main.udev_assign_seat("seat0").unwrap();
    let mut fn_pressed = false;
    // this one's actually output
    let actions = [
        [
            vec![Key::LeftMeta, Key::F8], // tap
            vec![Key::LeftMeta, Key::F11], // left
            vec![Key::LeftMeta, Key::F12], // right
        ],
        [
            vec![Key::Print],
            vec![Key::BrightnessDown],
            vec![Key::BrightnessUp],
        ],
        [
            vec![Key::LeftMeta, Key::F7],
            vec![Key::LeftMeta, Key::F9],
            vec![Key::LeftMeta, Key::F10],
        ],
    ];
    let fn_actions = [
        Key::F1, Key::F2, Key::F3, Key::F4,
        Key::F5, Key::F6, Key::F7, Key::F8,
        Key::F9, Key::F10, Key::F11, Key::F12,
    ];
    let mut key_down: Option<Key> = None;
    let mut uinput = UInputHandle::new(OpenOptions::new().write(true).open("/dev/uinput")?);
    uinput.set_evbit(EventKind::Key)?;
    for k in Key::iter() {
        uinput.set_keybit(k)?;
    }
    let mut dev_name_c = [0 as c_char; 80];
    let dev_name = "Dynamic Function Row Virtual Input Device".as_bytes();
    for i in 0..dev_name.len() {
        dev_name_c[i] = dev_name[i] as c_char;
    }
    uinput
        .dev_setup(&uinput_setup {
            id: input_id {
                bustype: 0x19,
                vendor: 0x1209,
                product: 0x316E,
                version: 1,
            },
            ff_effects_max: 0,
            name: dev_name_c,
        })?;
    uinput.dev_create()?;

    // random fields
    let mut rng = rand::rng();
    let perlin = Perlin::new(42);
    let mut i: f64 = 0.0;
    let mut ts: [(f64, f64); NUM] = [(0.0, 0.0); _];
    for i in 0..NUM {
        ts[i].0 = (i % (width as usize / F) * F) as f64;
        ts[i].1 = (i / (width as usize / F) * F) as f64;
    }

    // touch stuff
    let mut down: Option<(f64, f64)> = None; // point at which touch down happened
    let mut prev: Option<(f64, f64)> = None; // last point at which a touch event happened
                                             // (down or motion)
    let mut fields: Vec<Field> = vec![];

    loop {
        {
            let mut map = drm.map()?;
            let buffer: &mut [[u8; 4]] = map.as_chunks_mut().0;
        
            // wow i love rust
            for t in <_ as AsMut<[(f64, f64)]>>::as_mut(&mut ts) {
                let theta = perlin.get([t.0 * S, t.1 * S, i * T]) * TAU;
                let (dx, dy) = theta.sin_cos();
                let (mut vx, mut vy) = (0.0, 0.0);
                for field in &fields {
                    let (lvx, lvy) = field.to_move(t.0, t.1);
                    vx += lvx;
                    vy += lvy;
                }
                t.0 += dx + vx + width as f64 + (rng.random::<f64>() - 0.5) * R;
                t.1 += dy + vy + height as f64 + (rng.random::<f64>() - 0.5) * R;
                t.0 %= width as f64;
                t.1 %= height as f64;
    
                let color = Okhsva::new::<f32>(i as f32, 0.6, 1.0, 1.0);
                let rgbcolor: Srgba<u8> = Srgba::from_color(color).into_format();
                let bytecolor: [u8; 4] = rgbcolor.into();

                dot(buffer, width, height, t.0 as u32, t.1 as u32, bytecolor);
            }

            if fn_pressed {
                let l = real_height as f64 / 12.0;
                for n in 0..12 {
                    let y = (l * n as f64) as i32;
                    for x in 0..real_width {
                        for dy in -1i32..=1 {
                            dot(buffer, width, height, x as u32, (y + dy) as u32, [0, 0, 0, 0]);
                        }
                    }
                }
            }
        }

        drm.dirty(&[ClipRect::new(0, 0, width as u16, height as u16)])?;
        i += 1.0;

        fields = fields
            .iter()
            .filter_map(|f| if f.t < 100.0 {Some(f.update())} else {None})
            .collect();
        
        input_tb.dispatch()?;
        input_main.dispatch()?;
        for event in &mut input_main.clone() {
            if let Event::Keyboard(KeyboardEvent::Key(key)) = event {
                if key.key() == Key::Fn as u32 {
                    fn_pressed = key.key_state() == KeyState::Pressed;
                }
            }
        }
        for event in &mut input_tb.clone() {
            if let Event::Touch(te) = event {
                match te {
                    TouchEvent::Down(dn) => {
                        let x = dn.y_transformed(width as u32);
                        let y = dn.x_transformed(height as u32);
                        down = Some((x, y));
                        prev = down;
                        
                        if fn_pressed {
                            let key = fn_actions[(y / height as f64 * 12.0) as usize];
                            toggle_keys(&mut uinput, &vec![key], true as i32);
                            key_down = Some(key);
                            continue;
                        }
                    }
                    TouchEvent::Motion(mtn) => {
                        let x = mtn.y_transformed(width as u32);
                        let y = mtn.x_transformed(height as u32);

                        if let Some(real_prev) = prev {
                            let dx = x - real_prev.0;
                            let dy = y - real_prev.1;
                            let vx = -dx / 4.0;
                            let vy = dy / 4.0;

                            fields.push(Field {
                                t: 0.0,
                                x,
                                y,
                                vx,
                                vy,
                            });
                        }

                        prev = Some((x, y));
                    }
                    TouchEvent::Up(_) => {
                        if let Some(key) = key_down {
                            toggle_keys(&mut uinput, &vec![key], false as i32);
                            key_down = None;
                            continue;
                        }

                        // TouchUpEvent does not implement TouchEventPosition,
                        // so i use the previously recorded position instead
                        if let (Some(real_prev), Some(real_down)) = (prev, down) {
                            // we really only care about the y direction,
                            // because that's horizontal
                            let dy = real_prev.1 - real_down.1;
                            let region = height as f64 / 3.0;
                            let pr = real_prev.1 / region;
                            let dr = real_down.1 / region;
                            let prf = pr.floor();
                            let drf = dr.floor();
                            let a = &actions[clamp(prf as usize, 0, 2)];
                            if dy.abs() < 100.0 {
                                if !((pr > 0.9 && pr < 1.1) || (pr > 1.9 && pr < 2.1)) {
                                    println!("tap   {prf}");
                                    press_keys(&mut uinput, &a[0]);
                                } else {
                                    println!("ambiguous tap");
                                }
                            } else {
                                if prf == drf {
                                    if pr < dr {
                                        println!("left  {prf}");
                                        press_keys(&mut uinput, &a[1]);
                                    } else {
                                        println!("right {prf}");
                                        press_keys(&mut uinput, &a[2]);
                                    }
                                } else {
                                    println!("ambiguous swipe");
                                }
                            }
                        }

                        down = None;
                        prev = None;
                    }
                    _ => {}
                }
            }
        }
    }

    Ok(())
}
