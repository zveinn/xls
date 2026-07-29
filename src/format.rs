//! Modern single-line column rendering.
//!
//! Visual language (inspired by tools like eza / lsd / modern TUIs):
//! - columns separated by a dim hairline `│`
//! - type glyph before names (`▸` dir, `›` exec, `↗` link, …)
//! - permissions as type + triads: `d rwx·r-x·r-x`
//! - empty optional fields as an em dash `—`
//! - sparse as a filled/empty diamond
//! - size with a slightly brighter number, quieter unit

use std::io::{self, Write};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::columns::Column;
use crate::entry::{Entry, Kind};

pub const WHITE: &str = "\x1b[97m";
pub const BOLD_WHITE: &str = "\x1b[1;97m";
pub const LIGHT_BLUE: &str = "\x1b[38;5;117m";
pub const GREEN: &str = "\x1b[92m";
pub const RED: &str = "\x1b[91m";
pub const ORANGE: &str = "\x1b[38;5;214m";
pub const DIM: &str = "\x1b[90m";
pub const SOFT: &str = "\x1b[38;5;247m";
pub const SOFT_BLUE: &str = "\x1b[38;5;111m";
pub const HIDDEN_FILE: &str = "\x1b[38;5;87m";
pub const HIDDEN_DIR: &str = "\x1b[38;5;141m";
pub const HIDDEN_EXEC: &str = "\x1b[38;5;227m";
pub const HIDDEN_LINK: &str = "\x1b[38;5;213m";
pub const RESET: &str = "\x1b[0m";

/// Dim column separator between fields.
const SEP: &str = "\x1b[90m │ \x1b[0m";

#[derive(Default)]
pub struct Widths {
    perms: usize,
    nlink: usize,
    user: usize,  // "rwx name"
    group: usize, // "r-x name"
    other: usize, // "r-x" (+ markers)
    size: usize,
    blocks: usize,
    ino: usize,
    dev: usize,
    time: usize,
    flags: usize,
    xattrs: usize,
    xfs: usize,
    ty: usize,
    name: usize,
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
                Column::Other => w.other = w.other.max("OTHER".len()),
                Column::Size => w.size = w.size.max("SIZE".len()),
                Column::Nlink => w.nlink = w.nlink.max("N".len()),
                Column::Blocks => w.blocks = w.blocks.max("BLOCKS".len()),
                Column::Ino => w.ino = w.ino.max("INO:IGEN".len()),
                Column::Dev => w.dev = w.dev.max("DEV".len()),
                Column::Flags => w.flags = w.flags.max("FLAGS".len()),
                Column::Xattrs => w.xattrs = w.xattrs.max("XATTRS".len()),
                Column::Xfs => w.xfs = w.xfs.max("XFS".len()),
                Column::Name => w.name = w.name.max("NAME".len()),
                Column::Type => w.ty = w.ty.max("TYPE".len()),
                Column::Mtime
                | Column::Atime
                | Column::Ctime
                | Column::Birth
                | Column::Sparse => {}
            }
        }

        for e in entries {
            for c in cols {
                match c {
                    Column::Perms => w.perms = w.perms.max(perms_plain(e).chars().count()),
                    Column::User => {
                        w.user = w.user.max(owner_plain(&triad_user(e), &e.user).chars().count())
                    }
                    Column::Group => {
                        w.group = w
                            .group
                            .max(owner_plain(&triad_group(e), &e.group).chars().count())
                    }
                    Column::Other => w.other = w.other.max(other_plain(e).chars().count()),
                    Column::Size => w.size = w.size.max(human_size(e.size).len()),
                    Column::Nlink => w.nlink = w.nlink.max(e.nlink.to_string().len()),
                    Column::Blocks => w.blocks = w.blocks.max(format_blocks(e).len()),
                    Column::Ino => w.ino = w.ino.max(format_ino(e).len()),
                    Column::Dev => w.dev = w.dev.max(format_dev(e).len()),
                    Column::Flags => w.flags = w.flags.max(format_list_field(&e.extras.flags).len()),
                    Column::Xattrs => {
                        w.xattrs = w.xattrs.max(format_list_field_owned(&e.extras.xattrs).len())
                    }
                    Column::Xfs => w.xfs = w.xfs.max(format_xfs(e).len()),
                    Column::Type => w.ty = w.ty.max(type_word(e).len()),
                    Column::Name => {
                        w.name = w.name.max(e.name.chars().count());
                    }
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
            Column::Other => self.other,
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
            Column::Type => self.ty,
            Column::Name => self.name,
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

/// Full type word for the TYPE column.
fn type_word(e: &Entry) -> &'static str {
    if e.broken_symlink {
        return "broken";
    }
    match e.kind {
        Kind::Dir => "dir",
        Kind::Symlink => "link",
        Kind::Fifo => "fifo",
        Kind::Socket => "sock",
        Kind::Block => "block",
        Kind::Char => "char",
        Kind::File if e.executable => "exec",
        Kind::File => "file",
        Kind::Unknown => "unknown",
    }
}

pub fn write_header(out: &mut impl Write, cols: &[Column], w: &Widths) -> io::Result<()> {
    for (i, c) in cols.iter().enumerate() {
        if i > 0 {
            write!(out, "{SEP}")?;
        }
        let label = c.header();
        let width = w.width_for(*c);
        if matches!(c, Column::Size | Column::Nlink) {
            write!(out, "{BOLD_WHITE}{label:>width$}{RESET}")?;
        } else if width == 0 {
            write!(out, "{BOLD_WHITE}{label}{RESET}")?;
        } else {
            write!(out, "{BOLD_WHITE}{label:<width$}{RESET}")?;
        }
    }
    writeln!(out)?;
    write_header_rule(out, cols, w)
}

fn write_header_rule(out: &mut impl Write, cols: &[Column], w: &Widths) -> io::Result<()> {
    for (i, c) in cols.iter().enumerate() {
        if i > 0 {
            write!(out, "{DIM}─┼─{RESET}")?;
        }
        let width = w.width_for(*c).max(c.header().len());
        // Sparse is 1 wide; name uses measured width.
        let width = if *c == Column::Sparse { 1 } else { width };
        for _ in 0..width {
            write!(out, "{DIM}─{RESET}")?;
        }
    }
    writeln!(out)
}

pub fn write_entry(out: &mut impl Write, e: &Entry, cols: &[Column], w: &Widths) -> io::Result<()> {
    for (i, c) in cols.iter().enumerate() {
        if i > 0 {
            write!(out, "{SEP}")?;
        }
        write_column(out, e, *c, w)?;
    }
    writeln!(out)
}

fn write_column(out: &mut impl Write, e: &Entry, c: Column, w: &Widths) -> io::Result<()> {
    match c {
        Column::Mtime => write_time(out, e.mtime, w.time),
        Column::Atime => write_time(out, e.atime, w.time),
        Column::Ctime => write_time(
            out,
            epoch_to_system(e.ctime_secs),
            w.time,
        ),
        Column::Birth => write_time(out, e.birth, w.time),
        Column::Perms => write_perms(out, e, w.perms),
        Column::User => write_owner_col(out, &triad_user(e), &e.user, w.user),
        Column::Group => write_owner_col(out, &triad_group(e), &e.group, w.group),
        Column::Other => write_other_col(out, e, w.other),
        Column::Size => write_size(out, e.size, w.size),
        Column::Type => write_type(out, e, w.width_for(Column::Type)),
        Column::Name => write_name(out, e, w.name),
        Column::Nlink => write!(
            out,
            "{SOFT}{v:>width$}{RESET}",
            v = e.nlink,
            width = w.nlink
        ),
        Column::Blocks => write!(
            out,
            "{DIM}{v:<width$}{RESET}",
            v = format_blocks(e),
            width = w.blocks
        ),
        Column::Sparse => {
            if e.sparse {
                write!(out, "{ORANGE}◆{RESET}")
            } else {
                write!(out, "{DIM}◇{RESET}")
            }
        }
        Column::Ino => write_ino(out, e, w.ino),
        Column::Dev => write!(
            out,
            "{DIM}{v:<width$}{RESET}",
            v = format_dev(e),
            width = w.dev
        ),
        Column::Flags => write_badge(out, &format_list_field(&e.extras.flags), w.flags),
        Column::Xattrs => write_badge(
            out,
            &format_list_field_owned(&e.extras.xattrs),
            w.xattrs,
        ),
        Column::Xfs => write_badge(out, &format_xfs(e), w.xfs),
    }
}

fn write_time(out: &mut impl Write, t: Option<SystemTime>, width: usize) -> io::Result<()> {
    let plain = fmt_time_short(t);
    // Split date / time for a two-tone look when well-formed.
    if let Some((date, time)) = plain.split_once(' ') {
        write!(out, "{DIM}{date}{RESET} {SOFT}{time}{RESET}")?;
        let used = plain.chars().count();
        pad(out, width.saturating_sub(used))?;
    } else {
        write!(out, "{DIM}{plain:<width$}{RESET}")?;
    }
    Ok(())
}

fn write_size(out: &mut impl Write, n: u64, width: usize) -> io::Result<()> {
    let plain = human_size(n);
    // Right-align the whole token, but paint unit quieter.
    let pad_n = width.saturating_sub(plain.len());
    for _ in 0..pad_n {
        write!(out, " ")?;
    }
    if let Some(i) = plain.find(|c: char| c.is_ascii_alphabetic()) {
        write!(out, "{SOFT}{}{RESET}{DIM}{}{RESET}", &plain[..i], &plain[i..])?;
    } else {
        write!(out, "{SOFT}{plain}{RESET}")?;
    }
    Ok(())
}

fn write_ino(out: &mut impl Write, e: &Entry, width: usize) -> io::Result<()> {
    let plain = format_ino(e);
    if let Some((ino, igen)) = plain.split_once(':') {
        write!(out, "{SOFT}{ino}{RESET}{DIM}:{igen}{RESET}")?;
        pad(out, width.saturating_sub(plain.len()))?;
    } else {
        write!(out, "{DIM}{plain:<width$}{RESET}")?;
    }
    Ok(())
}

fn write_badge(out: &mut impl Write, text: &str, width: usize) -> io::Result<()> {
    if text == "—" {
        write!(out, "{DIM}{text:<width$}{RESET}")
    } else {
        // Soft “chip” feel without true background colors (portable).
        write!(out, "{ORANGE}{text:<width$}{RESET}")
    }
}

/// `rwx  sveinn` — triad then identity.
fn write_owner_col(
    out: &mut impl Write,
    triad: &str,
    name: &str,
    width: usize,
) -> io::Result<()> {
    write_triad(out, triad)?;
    write!(out, " {SOFT_BLUE}{name}{RESET}")?;
    let used = owner_plain(triad, name).chars().count();
    pad(out, width.saturating_sub(used))
}

/// Other class: triad + optional ACL/xattr markers (no identity name).
fn write_other_col(out: &mut impl Write, e: &Entry, width: usize) -> io::Result<()> {
    let plain = other_plain(e);
    write_triad(out, &triad_other(e))?;
    if e.extras.has_acl {
        write!(out, "{LIGHT_BLUE}+{RESET}")?;
    } else if !e.extras.xattrs.is_empty() {
        write!(out, "{ORANGE}@{RESET}")?;
    }
    pad(out, width.saturating_sub(plain.chars().count()))
}

fn write_triad(out: &mut impl Write, triad: &str) -> io::Result<()> {
    for ch in triad.chars() {
        match ch {
            'r' => write!(out, "{WHITE}r{RESET}")?,
            'w' => write!(out, "{RED}w{RESET}")?,
            'x' => write!(out, "{GREEN}x{RESET}")?,
            's' | 'S' | 't' | 'T' => write!(out, "{ORANGE}{ch}{RESET}")?,
            '-' => write!(out, "{DIM}-{RESET}")?,
            other => write!(out, "{DIM}{other}{RESET}")?,
        }
    }
    Ok(())
}

fn write_perms(out: &mut impl Write, e: &Entry, width: usize) -> io::Result<()> {
    // Optional classic full mode string for `--columns PERMS`.
    let plain = perms_plain(e);
    let type_color = color_for(e);
    let mut chars = plain.chars();

    if let Some(t) = chars.next() {
        write!(out, "{type_color}{t}{RESET}")?;
    }
    if chars.next() == Some(' ') {
        write!(out, " ")?;
    }

    for ch in chars {
        match ch {
            'r' => write!(out, "{WHITE}r{RESET}")?,
            'w' => write!(out, "{RED}w{RESET}")?,
            'x' => write!(out, "{GREEN}x{RESET}")?,
            's' | 'S' | 't' | 'T' => write!(out, "{ORANGE}{ch}{RESET}")?,
            '·' => write!(out, "{DIM}·{RESET}")?,
            '-' => write!(out, "{DIM}-{RESET}")?,
            '+' => write!(out, "{LIGHT_BLUE}+{RESET}")?,
            '@' => write!(out, "{ORANGE}@{RESET}")?,
            other => write!(out, "{DIM}{other}{RESET}")?,
        }
    }
    pad(out, width.saturating_sub(plain.chars().count()))
}

fn owner_plain(triad: &str, name: &str) -> String {
    format!("{triad} {name}")
}

fn triad_user(e: &Entry) -> String {
    bits_to_triad(e.mode, 0o400, 0o200, 0o100, Some(0o4000), false)
}

fn triad_group(e: &Entry) -> String {
    bits_to_triad(e.mode, 0o040, 0o020, 0o010, Some(0o2000), false)
}

fn triad_other(e: &Entry) -> String {
    bits_to_triad(e.mode, 0o004, 0o002, 0o001, None, true)
}

fn other_plain(e: &Entry) -> String {
    let mut s = triad_other(e);
    if e.extras.has_acl {
        s.push('+');
    } else if !e.extras.xattrs.is_empty() {
        s.push('@');
    }
    s
}

fn bits_to_triad(
    mode: u32,
    r: u32,
    w: u32,
    x: u32,
    special: Option<u32>,
    sticky: bool,
) -> String {
    let mut s = String::with_capacity(3);
    s.push(if mode & r != 0 { 'r' } else { '-' });
    s.push(if mode & w != 0 { 'w' } else { '-' });
    let exec = mode & x != 0;
    let ch = if sticky {
        let st = mode & 0o1000 != 0;
        match (exec, st) {
            (true, true) => 't',
            (false, true) => 'T',
            (true, false) => 'x',
            (false, false) => '-',
        }
    } else {
        let sp = special.is_some_and(|b| mode & b != 0);
        match (exec, sp) {
            (true, true) => 's',
            (false, true) => 'S',
            (true, false) => 'x',
            (false, false) => '-',
        }
    };
    s.push(ch);
    s
}

fn write_type(out: &mut impl Write, e: &Entry, width: usize) -> io::Result<()> {
    let color = color_for(e);
    let word = type_word(e);
    write!(out, "{color}{word:<width$}{RESET}")
}

fn write_name(out: &mut impl Write, e: &Entry, width: usize) -> io::Result<()> {
    let color = color_for(e);
    write!(out, "{color}{name}{RESET}", name = e.name)?;

    let mut used = e.name.chars().count();
    if let Some(target) = &e.symlink {
        let tc = if e.broken_symlink { RED } else { ORANGE };
        write!(out, " {DIM}→{RESET} {tc}{target}{RESET}")?;
        used = width; // skip trailing pad for long targets
    }
    pad(out, width.saturating_sub(used))
}

/// `d rwx·r-x·r-x` (+ optional ACL/xattr marker).
fn perms_plain(e: &Entry) -> String {
    let classic = e.mode_string();
    let mut chars = classic.chars();
    let t = chars.next().unwrap_or('?');
    let rest: String = chars.collect();
    // rest is 9 permission bits, optionally +/@
    let (bits, extra) = if rest.ends_with('+') || rest.ends_with('@') {
        let mut b = rest.clone();
        let ex = b.pop().unwrap();
        (b, Some(ex))
    } else {
        (rest, None)
    };
    let u = bits.get(0..3).unwrap_or("---");
    let g = bits.get(3..6).unwrap_or("---");
    let o = bits.get(6..9).unwrap_or("---");
    let mut s = format!("{t} {u}·{g}·{o}");
    if let Some(ex) = extra {
        s.push(ex);
    }
    s
}

fn format_blocks(e: &Entry) -> String {
    format!("{}b/{}", e.blocks, e.blksize)
}

fn format_ino(e: &Entry) -> String {
    let igen = e
        .extras
        .inode_gen
        .map(|g| g.to_string())
        .unwrap_or_else(|| "—".into());
    format!("{}:{}", e.ino, igen)
}

fn format_dev(e: &Entry) -> String {
    let mut s = format!("{}:{}", e.dev_major, e.dev_minor);
    if matches!(e.kind, Kind::Block | Kind::Char) {
        s.push_str(&format!(" ▸ {}:{}", e.rdev_major, e.rdev_minor));
    }
    s
}

fn format_xfs(e: &Entry) -> String {
    match e.xfs() {
        None => "—".into(),
        Some(x) => {
            let flags = if x.xflags.is_empty() {
                "—".into()
            } else {
                x.xflags.join(",")
            };
            let mut s = format!(
                "{flags} · exts={} · proj={} · esz={} · cow={}",
                x.nextents, x.projid, x.extsize, x.cowextsize
            );
            if let (Some(mem), Some(min), Some(max)) = (x.dio_mem, x.dio_min, x.dio_max) {
                s.push_str(&format!(" · dio={mem}/{min}/{max}"));
            }
            s
        }
    }
}

fn format_list_field(items: &[&str]) -> String {
    if items.is_empty() {
        "—".into()
    } else {
        items.join(" · ")
    }
}

fn format_list_field_owned(items: &[String]) -> String {
    if items.is_empty() {
        "—".into()
    } else {
        items.join(" · ")
    }
}

fn pad(out: &mut impl Write, n: usize) -> io::Result<()> {
    for _ in 0..n {
        write!(out, " ")?;
    }
    Ok(())
}

fn epoch_to_system(secs: i64) -> Option<SystemTime> {
    if secs <= 0 {
        None
    } else {
        Some(UNIX_EPOCH + Duration::from_secs(secs as u64))
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

