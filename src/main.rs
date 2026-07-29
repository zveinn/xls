mod entry;
mod format;
mod sort;
mod sys;

use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use entry::Entry;
use format::{
    DIM, GREEN, HIDDEN_DIR, HIDDEN_EXEC, HIDDEN_FILE, HIDDEN_LINK, LIGHT_BLUE, ORANGE, RED, RESET,
    WHITE, Mode, Widths, write_entry, write_header,
};
use sort::{SortKey, sort_entries};

enum Cli {
    Help,
    List {
        mode: Mode,
        path: PathBuf,
        sort: Option<SortKey>,
        headers: bool,
    },
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    match parse_args(&args) {
        Ok(Cli::Help) => {
            print_help();
            ExitCode::SUCCESS
        }
        Ok(Cli::List {
            mode,
            path,
            sort,
            headers,
        }) => {
            if let Err(e) = run(mode, &path, sort, headers) {
                eprintln!("{RED}xls: {e}{RESET}");
                return ExitCode::FAILURE;
            }
            ExitCode::SUCCESS
        }
        Err(msg) => {
            eprintln!("{RED}xls: {msg}{RESET}");
            eprintln!("Try '{WHITE}xls --help{RESET}' for more information.");
            ExitCode::FAILURE
        }
    }
}

fn parse_args(args: &[String]) -> Result<Cli, String> {
    let mut mode = Mode::Basic;
    let mut path = None;
    let mut help = false;
    let mut sort = None;
    let mut headers = true;
    let mut i = 0;

    while i < args.len() {
        let a = args[i].as_str();
        match a {
            "-a" => {
                if mode != Mode::Full {
                    mode = Mode::All;
                }
            }
            "-f" => mode = Mode::Full,
            "-af" | "-fa" => mode = Mode::Full,
            "-h" | "--help" => help = true,
            "--noHeaders" | "--no-headers" => headers = false,
            "--sort" => {
                i += 1;
                let Some(field) = args.get(i) else {
                    return Err("--sort requires a header name (e.g. --sort MTIME)".into());
                };
                sort = Some(SortKey::parse(field)?);
            }
            s if let Some(field) = s.strip_prefix("--sort=") => {
                if field.is_empty() {
                    return Err("--sort requires a header name (e.g. --sort=MTIME)".into());
                }
                sort = Some(SortKey::parse(field)?);
            }
            s if s.starts_with('-') => return Err(format!("unknown flag {s}")),
            s => {
                if path.is_some() {
                    return Err("only one path is supported".into());
                }
                path = Some(PathBuf::from(s));
            }
        }
        i += 1;
    }

    if help {
        return Ok(Cli::Help);
    }

    Ok(Cli::List {
        mode,
        path: path.unwrap_or_else(|| PathBuf::from(".")),
        sort,
        headers,
    })
}

fn print_help() {
    let h = LIGHT_BLUE;
    let k = WHITE;
    let d = DIM;
    let o = ORANGE;
    let fields = SortKey::names().join(", ");

    println!(
        "\
{h}xls{RESET} — colored directory listing

{h}USAGE{RESET}
  {k}xls{RESET} [{k}-a{RESET}|{k}-f{RESET}] [{k}--sort{RESET} {k}HEADER{RESET}] [{k}--noHeaders{RESET}] [{k}path{RESET}]
  {k}xls{RESET} [{k}-h{RESET}|{k}--help{RESET}]

{h}OPTIONS{RESET}
  {k}(none){RESET}            Basic listing: perms, user, group, size, mtime, name
  {k}-a{RESET}                All portable metadata (not filesystem-specific)
  {k}-f{RESET}                Full listing: everything in {k}-a{RESET} plus cheap XFS fields
  {k}--sort{RESET} {k}HEADER{RESET}     Sort by column header (always ascending)
  {k}--noHeaders{RESET}        Do not print the column header row
  {k}-h{RESET}, {k}--help{RESET}        Show this help and exit

{h}SORTING{RESET}
  Use {k}--sort HEADER{RESET} or {k}--sort=HEADER{RESET}. Names are case-insensitive.
  Order is always {o}ascending{RESET} (smallest / oldest / A–Z first).
  Ties break on {k}NAME{RESET} ascending.

  Sortable headers:
    {k}{fields}{RESET}

  Notes:
    {k}SIZE{RESET}, {k}N{RESET}, {k}BLOCKS{RESET}, {k}INO:IGEN{RESET}, {k}DEV{RESET}  numeric (low → high)
    {k}MTIME{RESET}, {k}ATIME{RESET}, {k}CTIME{RESET}, {k}BIRTH{RESET}   oldest first
    {k}NAME{RESET}, {k}USER{RESET}, {k}GROUP{RESET}, {k}PERMS{RESET}     lexicographic A–Z
    {k}S{RESET}                               non-sparse first
    {k}FLAGS{RESET}, {k}XATTRS{RESET}                    by content / count
    {k}XFS{RESET}                              by extent count, then project id
    Aliases: {d}NLINK/LINKS{RESET}→N, {d}INODE{RESET}→INO:IGEN, {d}OWNER{RESET}→USER, …

{h}COLORS{RESET}
  {WHITE}white{RESET}        regular file
  {LIGHT_BLUE}light blue{RESET}   directory (and headers)
  {GREEN}green{RESET}        executable
  {o}orange{RESET}       symlink / special file
  {RED}red{RESET}          error or broken symlink
  {HIDDEN_FILE}cyan{RESET}         hidden file ({d}.name{RESET})
  {HIDDEN_DIR}violet{RESET}       hidden directory ({d}.dir{RESET})
  {HIDDEN_EXEC}yellow{RESET}       hidden executable
  {HIDDEN_LINK}pink{RESET}         hidden symlink / special

{h}HEADERS — basic{RESET}  ({k}xls{RESET})
  {k}PERMS{RESET}   Classic mode string (color-coded), e.g. {d}drwxr-xr-x{RESET}
                    type char matches entry color; {WHITE}r{RESET} read,
                    {RED}w{RESET} write, {GREEN}x{RESET} execute,
                    {ORANGE}s/S/t/T{RESET} setuid/setgid/sticky,
                    {d}-{RESET} unset; trailing {LIGHT_BLUE}+{RESET} ACL, {ORANGE}@{RESET} xattrs
  {k}USER{RESET}    Owner user name
  {k}GROUP{RESET}   Owner group name
  {k}SIZE{RESET}    Logical size (human-readable: B/K/M/G/T)
  {k}MTIME{RESET}   Last content modification time (UTC, DD-MM-YYYY HH:MM:SS)
  {k}NAME{RESET}    Entry name (color indicates type); symlinks show {d}->{RESET} target

{h}HEADERS — all{RESET}  ({k}xls -a{RESET}; includes basic columns)
  {k}N{RESET}       Hard link count
  {k}BLOCKS{RESET}  Allocated blocks and preferred I/O block size
                    format: {d}<st_blocks>b/<blksize>{RESET}
                    ({d}st_blocks{RESET} is in 512-byte units)
  {k}S{RESET}       Sparse file flag: {d}S{RESET} if allocated < logical size, else {d}-{RESET}
  {k}INO:IGEN{RESET}  Inode number and inode generation (when available)
  {k}DEV{RESET}     Device id containing the file ({d}major:minor{RESET});
                    device nodes also show {d}rdev=major:minor{RESET}
  {k}ATIME{RESET}   Last access time (UTC; may be stale on noatime mounts)
  {k}CTIME{RESET}   Last status-change time (metadata change, not create)
  {k}BIRTH{RESET}   Creation / birth time when the filesystem provides it
  {k}FLAGS{RESET}   Linux inode flags from {d}FS_IOC_GETFLAGS{RESET}
                    (e.g. immutable, append, noatime, dax, …) or {d}-{RESET}
  {k}XATTRS{RESET}  Extended attribute names, comma-separated, or {d}-{RESET}

{h}HEADERS — full{RESET}  ({k}xls -f{RESET}; includes all columns)
  {k}XFS{RESET}     Cheap XFS inode info from {d}FS_IOC_FSGETXATTR{RESET}
                    and {d}XFS_IOC_DIOINFO{RESET}:
                      {d}xflags{RESET}  e.g. hasattr, prealloc, dax, …
                      {d}exts{RESET}    number of extents
                      {d}proj{RESET}    project id
                      {d}esz{RESET}     extent-size hint
                      {d}cow{RESET}     CoW extent-size hint
                      {d}dio{RESET}     direct-I/O align/min/max (when set)
                    Shows {d}-{RESET} when XFS ioctls are unavailable

{h}EXAMPLES{RESET}
  {k}xls{RESET}
  {k}xls /var/log{RESET}
  {k}xls -a .{RESET}
  {k}xls -f /home{RESET}
  {k}xls --sort SIZE{RESET}
  {k}xls -a --sort MTIME{RESET}
  {k}xls -f --sort=GROUP /var{RESET}
"
    );
}

fn run(mode: Mode, path: &Path, sort: Option<SortKey>, headers: bool) -> io::Result<()> {
    let detail = match mode {
        Mode::Basic => 0,
        Mode::All => 1,
        Mode::Full => 2,
    };
    // Collect enough metadata for the chosen sort key even if not displayed.
    let detail = sort.map(|k| detail.max(k.min_detail())).unwrap_or(detail);

    let meta = fs::symlink_metadata(path)?;

    let mut entries = if meta.is_dir() {
        let mut v = Vec::new();
        for ent in fs::read_dir(path)? {
            let ent = ent?;
            let name = ent.file_name().to_string_lossy().into_owned();
            match Entry::collect(ent.path(), name, detail) {
                Ok(e) => v.push(e),
                Err(err) => eprintln!("{RED}xls: {}: {err}{RESET}", ent.path().display()),
            }
        }
        v
    } else {
        let name = path
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.display().to_string());
        vec![Entry::collect(path.to_path_buf(), name, detail)?]
    };

    match sort {
        Some(key) => sort_entries(&mut entries, key),
        None => entries.sort_by(|a, b| {
            a.name
                .to_ascii_lowercase()
                .cmp(&b.name.to_ascii_lowercase())
        }),
    }

    let widths = Widths::measure(&entries, mode);
    let mut out = io::stdout().lock();
    if headers {
        write_header(&mut out, mode, &widths)?;
    }
    for e in &entries {
        write_entry(&mut out, e, mode, &widths)?;
    }
    out.flush()
}
