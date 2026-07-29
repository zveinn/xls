//! Colored one-line rendering for every mode.

use std::io::{self, Write};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::entry::{Entry, Kind};

pub const WHITE: &str = "\x1b[97m";
pub const LIGHT_BLUE: &str = "\x1b[38;5;117m";
pub const GREEN: &str = "\x1b[92m";
pub const RED: &str = "\x1b[91m";
pub const ORANGE: &str = "\x1b[38;5;214m";
pub const DIM: &str = "\x1b[90m";
/// Slightly brighter than DIM — used for SIZE so it stays secondary to names.
pub const SOFT: &str = "\x1b[38;5;247m";
/// Hidden (dot) entries — distinct colors from the main palette.
pub const HIDDEN_FILE: &str = "\x1b[38;5;87m"; // cyan
pub const HIDDEN_DIR: &str = "\x1b[38;5;141m"; // violet
pub const HIDDEN_EXEC: &str = "\x1b[38;5;227m"; // yellow
pub const HIDDEN_LINK: &str = "\x1b[38;5;213m"; // pink
pub const RESET: &str = "\x1b[0m";

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// mode size mtime name
    Basic,
    /// + owner, links, times, inode, flags, xattrs, …
    All,
    /// + cheap XFS fields
    Full,
}

#[derive(Default)]
pub struct Widths {
    mode: usize,
    nlink: usize,
    user: usize,
    group: usize,
    size: usize,
    blocks: usize, // "Nb/blksize"
    ino: usize,    // "ino:igen"
    dev: usize,
    time: usize, // "DD-MM-YYYY HH:MM:SS"
    flags: usize,
    xattrs: usize,
    xfs: usize,
}

impl Widths {
    pub fn measure(entries: &[Entry], mode: Mode) -> Self {
        let mut w = Self {
            mode: "PERMS".len(),
            size: "SIZE".len(),
            nlink: "N".len(),
            user: "USER".len(),
            group: "GROUP".len(),
            blocks: "BLOCKS".len(),
            ino: "INO:IGEN".len(),
            dev: "DEV".len(),
            time: "DD-MM-YYYY HH:MM:SS".len(),
            flags: "FLAGS".len(),
            xattrs: "XATTRS".len(),
            xfs: "XFS".len(),
        };

        for e in entries {
            w.mode = w.mode.max(e.mode_string().len());
            w.size = w.size.max(human_size(e.size).len());
            w.user = w.user.max(e.user.len());
            w.group = w.group.max(e.group.len());
            if mode == Mode::Basic {
                continue;
            }
            w.nlink = w.nlink.max(e.nlink.to_string().len());
            w.blocks = w.blocks.max(format!("{}b/{}", e.blocks, e.blksize).len());
            w.ino = w.ino.max(format_ino(e).len());
            w.dev = w.dev.max(format_dev(e).len());
            w.flags = w.flags.max(join_or_dash(&e.extras.flags).len());
            let xa = if e.extras.xattrs.is_empty() {
                "-".to_string()
            } else {
                e.extras.xattrs.join(",")
            };
            w.xattrs = w.xattrs.max(xa.len());
            if mode == Mode::Full {
                w.xfs = w.xfs.max(format_xfs(e).len());
            }
        }
        w
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

pub fn write_header(out: &mut impl Write, mode: Mode, w: &Widths) -> io::Result<()> {
    match mode {
        Mode::Basic => writeln!(
            out,
            "{LIGHT_BLUE}{perms:<pw$} {user:<uw$} {group:<gw$} {mtime:<tw$} {size:>sw$} {name}{RESET}",
            perms = "PERMS",
            user = "USER",
            group = "GROUP",
            mtime = "MTIME",
            size = "SIZE",
            name = "NAME",
            pw = w.mode,
            uw = w.user,
            gw = w.group,
            tw = w.time,
            sw = w.size,
        ),
        Mode::All => write_detail_header(out, w, false),
        Mode::Full => write_detail_header(out, w, true),
    }
}

fn write_detail_header(out: &mut impl Write, w: &Widths, xfs: bool) -> io::Result<()> {
    write!(
        out,
        "{LIGHT_BLUE}{perms:<pw$} {nlink:>nw$} {user:<uw$} {group:<gw$} {size:>sw$} {blocks:<bw$} {sp} {ino:<iw$} {dev:<dw$} {mtime:<tw$} {atime:<tw$} {ctime:<tw$} {birth:<tw$} {flags:<fw$} {xattrs:<xw$}",
        perms = "PERMS",
        nlink = "N",
        user = "USER",
        group = "GROUP",
        size = "SIZE",
        blocks = "BLOCKS",
        sp = "S",
        ino = "INO:IGEN",
        dev = "DEV",
        mtime = "MTIME",
        atime = "ATIME",
        ctime = "CTIME",
        birth = "BIRTH",
        flags = "FLAGS",
        xattrs = "XATTRS",
        pw = w.mode,
        nw = w.nlink,
        uw = w.user,
        gw = w.group,
        sw = w.size,
        bw = w.blocks,
        iw = w.ino,
        dw = w.dev,
        tw = w.time,
        fw = w.flags,
        xw = w.xattrs,
    )?;
    if xfs {
        write!(out, " {xfs:<xw$}", xfs = "XFS", xw = w.xfs)?;
    }
    writeln!(out, " {name}{RESET}", name = "NAME")
}

pub fn write_entry(out: &mut impl Write, e: &Entry, mode: Mode, w: &Widths) -> io::Result<()> {
    match mode {
        Mode::Basic => write_basic(out, e, w),
        Mode::All => write_all(out, e, w, false),
        Mode::Full => write_all(out, e, w, true),
    }
}

fn write_basic(out: &mut impl Write, e: &Entry, w: &Widths) -> io::Result<()> {
    let color = color_for(e);
    let size = human_size(e.size);
    let mtime = fmt_time_short(e.mtime);

    write_perms(out, e, w.mode)?;
    write!(
        out,
        " {LIGHT_BLUE}{user:<uw$}{RESET} {LIGHT_BLUE}{group:<gw$}{RESET} {DIM}{mtime:<tw$}{RESET} {SOFT}{size:>sw$}{RESET} ",
        user = e.user,
        group = e.group,
        uw = w.user,
        gw = w.group,
        tw = w.time,
        sw = w.size,
    )?;
    write_name(out, e, color)?;
    writeln!(out)
}

fn write_all(out: &mut impl Write, e: &Entry, w: &Widths, xfs: bool) -> io::Result<()> {
    let color = color_for(e);
    let size = human_size(e.size);
    let nlink = e.nlink.to_string();
    let blocks = format!("{}b/{}", e.blocks, e.blksize);
    let ino = format_ino(e);
    let dev = format_dev(e);
    let flags = join_or_dash(&e.extras.flags);
    let xattrs = if e.extras.xattrs.is_empty() {
        "-".into()
    } else {
        e.extras.xattrs.join(",")
    };
    let sparse = if e.sparse { "S" } else { "-" };

    write_perms(out, e, w.mode)?;
    write!(
        out,
        " {WHITE}{nlink:>nw$}{RESET} {LIGHT_BLUE}{user:<uw$}{RESET} {LIGHT_BLUE}{group:<gw$}{RESET} {SOFT}{size:>sw$}{RESET} {DIM}{blocks:<bw$}{RESET} {DIM}{sparse}{RESET} {DIM}{ino:<iw$}{RESET} {DIM}{dev:<dw$}{RESET} {DIM}{mtime:<tw$}{RESET} {DIM}{atime:<tw$}{RESET} {DIM}{ctime:<tw$}{RESET} {DIM}{birth:<tw$}{RESET} {ORANGE}{flags:<fw$}{RESET} {ORANGE}{xattrs:<xw$}{RESET}",
        user = e.user,
        group = e.group,
        nw = w.nlink,
        uw = w.user,
        gw = w.group,
        sw = w.size,
        bw = w.blocks,
        iw = w.ino,
        dw = w.dev,
        tw = w.time,
        fw = w.flags,
        xw = w.xattrs,
        mtime = fmt_time_short(e.mtime),
        atime = fmt_time_short(e.atime),
        ctime = fmt_epoch_short(e.ctime_secs),
        birth = fmt_time_short(e.birth),
    )?;

    if xfs {
        let x = format_xfs(e);
        write!(out, " {ORANGE}{x:<xw$}{RESET}", xw = w.xfs)?;
    }

    write!(out, " ")?;
    write_name(out, e, color)?;
    writeln!(out)
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

/// Color-coded classic permissions (`drwxr-xr-x`), padded to `width`.
///
/// - type char: entry color
/// - `r` white · `w` red · `x` green · `s`/`S`/`t`/`T` orange · `-` dim
/// - `+` ACL light blue · `@` xattr orange
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

    // Howard Hinnant civil_from_days (UTC)
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
