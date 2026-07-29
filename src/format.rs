//! Column-driven colored rendering.

use std::io::{self, Write};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::columns::Column;
use crate::entry::{Entry, Kind};

pub const WHITE: &str = "\x1b[97m";
pub const LIGHT_BLUE: &str = "\x1b[38;5;117m";
pub const GREEN: &str = "\x1b[92m";
pub const RED: &str = "\x1b[91m";
pub const ORANGE: &str = "\x1b[38;5;214m";
pub const DIM: &str = "\x1b[90m";
/// Slightly brighter than DIM — used for SIZE so it stays secondary to names.
pub const SOFT: &str = "\x1b[38;5;247m";
pub const HIDDEN_FILE: &str = "\x1b[38;5;87m";
pub const HIDDEN_DIR: &str = "\x1b[38;5;141m";
pub const HIDDEN_EXEC: &str = "\x1b[38;5;227m";
pub const HIDDEN_LINK: &str = "\x1b[38;5;213m";
pub const RESET: &str = "\x1b[0m";

#[derive(Default)]
pub struct Widths {
    perms: usize,
    nlink: usize,
    user: usize,
    group: usize,
    size: usize,
    blocks: usize,
    ino: usize,
    dev: usize,
    time: usize,
    flags: usize,
    xattrs: usize,
    xfs: usize,
}

impl Widths {
    pub fn measure(entries: &[Entry], cols: &[Column]) -> Self {
        let mut w = Self {
            time: "DD-MM-YYYY HH:MM:SS".len(),
            ..Default::default()
        };

        for c in cols {
            match c {
                Column::Perms => w.perms = w.perms.max("PERMS".len()),
                Column::User => w.user = w.user.max("USER".len()),
                Column::Group => w.group = w.group.max("GROUP".len()),
                Column::Size => w.size = w.size.max("SIZE".len()),
                Column::Nlink => w.nlink = w.nlink.max("N".len()),
                Column::Blocks => w.blocks = w.blocks.max("BLOCKS".len()),
                Column::Ino => w.ino = w.ino.max("INO:IGEN".len()),
                Column::Dev => w.dev = w.dev.max("DEV".len()),
                Column::Flags => w.flags = w.flags.max("FLAGS".len()),
                Column::Xattrs => w.xattrs = w.xattrs.max("XATTRS".len()),
                Column::Xfs => w.xfs = w.xfs.max("XFS".len()),
                Column::Mtime
                | Column::Atime
                | Column::Ctime
                | Column::Birth
                | Column::Sparse
                | Column::Name => {}
            }
        }

        for e in entries {
            for c in cols {
                match c {
                    Column::Perms => w.perms = w.perms.max(e.mode_string().len()),
                    Column::User => w.user = w.user.max(e.user.len()),
                    Column::Group => w.group = w.group.max(e.group.len()),
                    Column::Size => w.size = w.size.max(human_size(e.size).len()),
                    Column::Nlink => w.nlink = w.nlink.max(e.nlink.to_string().len()),
                    Column::Blocks => {
                        w.blocks = w.blocks.max(format!("{}b/{}", e.blocks, e.blksize).len())
                    }
                    Column::Ino => w.ino = w.ino.max(format_ino(e).len()),
                    Column::Dev => w.dev = w.dev.max(format_dev(e).len()),
                    Column::Flags => w.flags = w.flags.max(join_or_dash(&e.extras.flags).len()),
                    Column::Xattrs => {
                        let xa = if e.extras.xattrs.is_empty() {
                            "-".to_string()
                        } else {
                            e.extras.xattrs.join(",")
                        };
                        w.xattrs = w.xattrs.max(xa.len());
                    }
                    Column::Xfs => w.xfs = w.xfs.max(format_xfs(e).len()),
                    _ => {}
                }
            }
        }
        w
    }

    fn width_for(&self, c: Column) -> usize {
        match c {
            Column::Perms => self.perms,
            Column::User => self.user,
            Column::Group => self.group,
            Column::Size => self.size,
            Column::Nlink => self.nlink,
            Column::Blocks => self.blocks,
            Column::Ino => self.ino,
            Column::Dev => self.dev,
            Column::Flags => self.flags,
            Column::Xattrs => self.xattrs,
            Column::Xfs => self.xfs,
            Column::Mtime | Column::Atime | Column::Ctime | Column::Birth => self.time,
            Column::Sparse => 1,
            Column::Name => 0,
        }
    }
}

pub fn color_for(e: &Entry) -> &'static str {
    if e.broken_symlink {
        return RED;
    }
    let hidden = e.name.starts_with('.');
    match e.kind {
        Kind::Dir if hidden => HIDDEN_DIR,
        Kind::Dir => LIGHT_BLUE,
        Kind::Symlink if hidden => HIDDEN_LINK,
        Kind::Symlink => ORANGE,
        Kind::File if e.executable && hidden => HIDDEN_EXEC,
        Kind::File if e.executable => GREEN,
        Kind::Fifo | Kind::Socket | Kind::Block | Kind::Char if hidden => HIDDEN_LINK,
        Kind::Fifo | Kind::Socket | Kind::Block | Kind::Char => ORANGE,
        Kind::File | Kind::Unknown if hidden => HIDDEN_FILE,
        Kind::File | Kind::Unknown => WHITE,
    }
}

pub fn write_header(out: &mut impl Write, cols: &[Column], w: &Widths) -> io::Result<()> {
    for (i, c) in cols.iter().enumerate() {
        if i > 0 {
            write!(out, " ")?;
        }
        let label = c.header();
        let width = w.width_for(*c);
        if width == 0 {
            write!(out, "{LIGHT_BLUE}{label}{RESET}")?;
        } else if matches!(c, Column::Size | Column::Nlink) {
            write!(out, "{LIGHT_BLUE}{label:>width$}{RESET}")?;
        } else {
            write!(out, "{LIGHT_BLUE}{label:<width$}{RESET}")?;
        }
    }
    writeln!(out)
}

pub fn write_entry(out: &mut impl Write, e: &Entry, cols: &[Column], w: &Widths) -> io::Result<()> {
    for (i, c) in cols.iter().enumerate() {
        if i > 0 {
            write!(out, " ")?;
        }
        write_column(out, e, *c, w)?;
    }
    writeln!(out)
}

fn write_column(out: &mut impl Write, e: &Entry, c: Column, w: &Widths) -> io::Result<()> {
    match c {
        Column::Mtime => write!(
            out,
            "{DIM}{v:<width$}{RESET}",
            v = fmt_time_short(e.mtime),
            width = w.time
        ),
        Column::Atime => write!(
            out,
            "{DIM}{v:<width$}{RESET}",
            v = fmt_time_short(e.atime),
            width = w.time
        ),
        Column::Ctime => write!(
            out,
            "{DIM}{v:<width$}{RESET}",
            v = fmt_epoch_short(e.ctime_secs),
            width = w.time
        ),
        Column::Birth => write!(
            out,
            "{DIM}{v:<width$}{RESET}",
            v = fmt_time_short(e.birth),
            width = w.time
        ),
        Column::Perms => write_perms(out, e, w.perms),
        Column::User => write!(
            out,
            "{LIGHT_BLUE}{v:<width$}{RESET}",
            v = e.user,
            width = w.user
        ),
        Column::Group => write!(
            out,
            "{LIGHT_BLUE}{v:<width$}{RESET}",
            v = e.group,
            width = w.group
        ),
        Column::Size => write!(
            out,
            "{SOFT}{v:>width$}{RESET}",
            v = human_size(e.size),
            width = w.size
        ),
        Column::Name => write_name(out, e, color_for(e)),
        Column::Nlink => write!(
            out,
            "{WHITE}{v:>width$}{RESET}",
            v = e.nlink,
            width = w.nlink
        ),
        Column::Blocks => write!(
            out,
            "{DIM}{v:<width$}{RESET}",
            v = format!("{}b/{}", e.blocks, e.blksize),
            width = w.blocks
        ),
        Column::Sparse => {
            let s = if e.sparse { "S" } else { "-" };
            write!(out, "{DIM}{s}{RESET}")
        }
        Column::Ino => write!(
            out,
            "{DIM}{v:<width$}{RESET}",
            v = format_ino(e),
            width = w.ino
        ),
        Column::Dev => write!(
            out,
            "{DIM}{v:<width$}{RESET}",
            v = format_dev(e),
            width = w.dev
        ),
        Column::Flags => write!(
            out,
            "{ORANGE}{v:<width$}{RESET}",
            v = join_or_dash(&e.extras.flags),
            width = w.flags
        ),
        Column::Xattrs => {
            let xa = if e.extras.xattrs.is_empty() {
                "-".to_string()
            } else {
                e.extras.xattrs.join(",")
            };
            write!(out, "{ORANGE}{v:<width$}{RESET}", v = xa, width = w.xattrs)
        }
        Column::Xfs => write!(
            out,
            "{ORANGE}{v:<width$}{RESET}",
            v = format_xfs(e),
            width = w.xfs
        ),
    }
}

fn format_ino(e: &Entry) -> String {
    let igen = e
        .extras
        .inode_gen
        .map(|g| g.to_string())
        .unwrap_or_else(|| "-".into());
    format!("{}:{}", e.ino, igen)
}

fn format_dev(e: &Entry) -> String {
    let mut s = format!("{}:{}", e.dev_major, e.dev_minor);
    if matches!(e.kind, Kind::Block | Kind::Char) {
        s.push_str(&format!(" rdev={}:{}", e.rdev_major, e.rdev_minor));
    }
    s
}

fn format_xfs(e: &Entry) -> String {
    match e.xfs() {
        None => "-".into(),
        Some(x) => {
            let flags = join_or_dash(&x.xflags);
            let mut s = format!(
                "{flags},exts={},proj={},esz={},cow={}",
                x.nextents, x.projid, x.extsize, x.cowextsize
            );
            if let (Some(mem), Some(min), Some(max)) = (x.dio_mem, x.dio_min, x.dio_max) {
                s.push_str(&format!(",dio={mem}/{min}/{max}"));
            }
            s
        }
    }
}

fn write_perms(out: &mut impl Write, e: &Entry, width: usize) -> io::Result<()> {
    let plain = e.mode_string();
    let type_color = color_for(e);

    for (i, ch) in plain.chars().enumerate() {
        let color = if i == 0 {
            type_color
        } else {
            match ch {
                'r' => WHITE,
                'w' => RED,
                'x' => GREEN,
                's' | 'S' | 't' | 'T' => ORANGE,
                '+' => LIGHT_BLUE,
                '@' => ORANGE,
                _ => DIM,
            }
        };
        write!(out, "{color}{ch}{RESET}")?;
    }

    let pad = width.saturating_sub(plain.chars().count());
    for _ in 0..pad {
        write!(out, " ")?;
    }
    Ok(())
}

fn write_name(out: &mut impl Write, e: &Entry, color: &str) -> io::Result<()> {
    write!(out, "{color}{name}{RESET}", name = e.name)?;
    if let Some(target) = &e.symlink {
        let tc = if e.broken_symlink { RED } else { ORANGE };
        write!(out, " {DIM}->{RESET} {tc}{target}{RESET}")?;
    }
    Ok(())
}

fn join_or_dash(items: &[&str]) -> String {
    if items.is_empty() {
        "-".into()
    } else {
        items.join(",")
    }
}

pub fn human_size(n: u64) -> String {
    const UNITS: [&str; 5] = ["B", "K", "M", "G", "T"];
    let mut v = n as f64;
    let mut i = 0;
    while v >= 1024.0 && i < UNITS.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{n}{}", UNITS[0])
    } else if v >= 10.0 {
        format!("{v:.0}{}", UNITS[i])
    } else {
        format!("{v:.1}{}", UNITS[i])
    }
}

fn fmt_time_short(t: Option<SystemTime>) -> String {
    match t.and_then(system_parts) {
        Some((y, mo, d, h, mi, s)) => format!("{d:02}-{mo:02}-{y:04} {h:02}:{mi:02}:{s:02}"),
        None => "—".into(),
    }
}

fn fmt_epoch_short(secs: i64) -> String {
    if secs <= 0 {
        return "—".into();
    }
    let t = UNIX_EPOCH + Duration::from_secs(secs as u64);
    fmt_time_short(Some(t))
}

fn system_parts(t: SystemTime) -> Option<(u64, u64, u64, u64, u64, u64)> {
    let secs = t.duration_since(UNIX_EPOCH).ok()?.as_secs();
    let z = secs / 86400;
    let tod = secs % 86400;
    let h = tod / 3600;
    let mi = (tod % 3600) / 60;
    let s = tod % 60;

    let z = z as i64 + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };

    Some((y as u64, m as u64, d, h, mi, s))
}
