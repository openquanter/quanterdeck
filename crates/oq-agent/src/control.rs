//! The trading process's control port, as the agent reaches it.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::time::Duration;

use oq_deck_core::lang::Said;
use serde_json::Value;

/// Every control socket in `dir`, by name: the file name without `.sock`,
/// sorted so the order is the same on every look.
///
/// # Errors
/// The directory cannot be read.
pub fn sockets(dir: &Path) -> Result<Vec<(String, PathBuf)>, Said> {
    use std::os::unix::fs::FileTypeExt;
    let mut socks: Vec<(String, PathBuf)> = std::fs::read_dir(dir)
        .map_err(|e| Said::same(format!("{}: {e}", dir.display())))?
        .filter_map(Result::ok)
        .filter(|e| std::fs::symlink_metadata(e.path()).is_ok_and(|m| m.file_type().is_socket()))
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            name.strip_suffix(".sock")
                .map(|id| (id.to_string(), e.path()))
        })
        .collect();
    socks.sort();
    Ok(socks)
}

fn none_running() -> Said {
    Said::new(
        "交易进程没有控制端口：它没有在运行，或者启动时没带。",
        "the trader has no control socket: it is not running, or runs without one",
    )
}

/// The control socket a request means: the one named `trader`, or the
/// only one when none is named.
///
/// A name is matched against the directory listing, never joined into a
/// path, so a request cannot reach a file the listing does not show.
///
/// # Errors
/// None there (the trader is not running, or has no port); a name that
/// is not one of them; or, with no name, more than one — which would mean
/// two traders, and a guess about which to command is not one to make.
pub fn socket_for(dir: &Path, trader: Option<&str>) -> Result<PathBuf, Said> {
    let socks = sockets(dir)?;
    if let Some(name) = trader {
        return socks
            .into_iter()
            .find(|(id, _)| id == name)
            .map(|(_, p)| p)
            .ok_or_else(|| {
                Said::new(
                    format!("没有名为 {name} 的交易进程控制端口"),
                    format!("no trader control socket is named {name}"),
                )
            });
    }
    match socks.len() {
        0 => Err(none_running()),
        1 => Ok(socks.into_iter().next().map(|(_, p)| p).unwrap_or_default()),
        n => Err(Said::new(
            format!(
                "{} 里有 {n} 个控制端口，不猜哪一个：请指明交易进程（{}）。",
                dir.display(),
                socks
                    .iter()
                    .map(|(id, _)| id.as_str())
                    .collect::<Vec<_>>()
                    .join("、")
            ),
            format!(
                "{n} control sockets in {}; refusing to choose — name the trader ({})",
                dir.display(),
                socks
                    .iter()
                    .map(|(id, _)| id.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        )),
    }
}

/// Keep a field to one line with no separators: it is written into a
/// tab-separated request.
fn clean(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c == '\t' || c == '\n' || c == '\r' {
                ' '
            } else {
                c
            }
        })
        .collect::<String>()
        .trim()
        .to_string()
}

/// Send one command and read the answer.
///
/// # Errors
/// No socket, a connection or read failure, or an answer that is not JSON.
pub fn ask(dir: &Path, command: &str, origin: &str, reason: &str) -> Result<Value, Said> {
    ask_trader(dir, None, command, origin, reason)
}

/// As [`ask`], of the trader named `trader`, or the only one.
///
/// # Errors
/// As [`ask`], and a name that is not one of the sockets there.
pub fn ask_trader(
    dir: &Path,
    trader: Option<&str>,
    command: &str,
    origin: &str,
    reason: &str,
) -> Result<Value, Said> {
    ask_at(&socket_for(dir, trader)?, command, origin, reason)
}

/// One trader's answer, by the name of its control socket.
pub type Look = (String, Result<Value, Said>);

/// Ask every trader in `dir` the same question: each socket's name and
/// its answer. Empty when none runs.
///
/// # Errors
/// The directory cannot be read.
pub fn ask_each(dir: &Path, command: &str, origin: &str) -> Result<Vec<Look>, Said> {
    Ok(sockets(dir)?
        .into_iter()
        .map(|(id, path)| {
            let answer = ask_at(&path, command, origin, "");
            (id, answer)
        })
        .collect())
}

fn ask_at(path: &Path, command: &str, origin: &str, reason: &str) -> Result<Value, Said> {
    let mut stream =
        UnixStream::connect(path).map_err(|e| Said::same(format!("{}: {e}", path.display())))?;
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .map_err(|e| Said::same(e.to_string()))?;
    stream
        .set_write_timeout(Some(Duration::from_secs(5)))
        .map_err(|e| Said::same(e.to_string()))?;
    writeln!(stream, "{command}\t{}\t{}", clean(origin), clean(reason))
        .map_err(|e| Said::same(format!("write: {e}")))?;
    let mut line = String::new();
    BufReader::new(&stream)
        .take_line(&mut line)
        .map_err(|e| Said::same(format!("read: {e}")))?;
    serde_json::from_str(&line).map_err(|e| {
        Said::new(
            format!("控制端口回了一段读不懂的内容：{e}"),
            format!("the port answered something unreadable: {e}"),
        )
    })
}

trait TakeLine {
    fn take_line(&mut self, line: &mut String) -> std::io::Result<usize>;
}

impl<R: BufRead> TakeLine for R {
    /// One line, at most 1 MiB: a status is small, and a port that sends
    /// without end is not one to keep reading.
    fn take_line(&mut self, line: &mut String) -> std::io::Result<usize> {
        let mut limited = std::io::Read::take(self, 1 << 20);
        let mut buf = Vec::new();
        let n = std::io::BufRead::read_until(&mut BufReader::new(&mut limited), b'\n', &mut buf)?;
        line.push_str(&String::from_utf8_lossy(&buf));
        Ok(n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::net::UnixListener;

    #[test]
    fn a_command_goes_out_as_one_clean_line_and_the_answer_comes_back() {
        let dir = tempfile::tempdir().expect("dir");
        let listener = UnixListener::bind(dir.path().join("oq-live.x.sock")).expect("bind");
        let server = std::thread::spawn(move || {
            let (s, _) = listener.accept().expect("accept");
            let mut line = String::new();
            BufReader::new(&s).read_line(&mut line).expect("read");
            let mut w = &s;
            writeln!(w, r#"{{"ok":true,"state":"halted"}}"#).expect("write");
            line
        });
        let answer = ask(dir.path(), "halt", "deck:a\tb", "a\nreason").expect("asks");
        assert_eq!(answer["state"], "halted");
        assert_eq!(server.join().expect("server"), "halt\tdeck:a b\ta reason\n");
    }

    #[test]
    fn no_socket_or_two_is_an_error_not_a_guess() {
        let dir = tempfile::tempdir().expect("dir");
        assert!(socket_for(dir.path(), None).is_err());
        let _a = UnixListener::bind(dir.path().join("a.sock")).expect("a");
        let _b = UnixListener::bind(dir.path().join("b.sock")).expect("b");
        let refused = socket_for(dir.path(), None).unwrap_err().en;
        assert!(refused.contains("refusing to choose"), "{refused}");
        assert!(refused.contains("a, b"), "and names them: {refused}");
    }

    #[test]
    fn a_named_trader_is_found_in_the_listing_never_joined_into_a_path() {
        let dir = tempfile::tempdir().expect("dir");
        let _a = UnixListener::bind(dir.path().join("oq-live.a.sock")).expect("a");
        let _b = UnixListener::bind(dir.path().join("oq-live.b.sock")).expect("b");
        std::fs::write(dir.path().join("not-a-socket.sock"), b"").expect("file");
        assert_eq!(
            socket_for(dir.path(), Some("oq-live.b")).expect("found"),
            dir.path().join("oq-live.b.sock")
        );
        for bad in ["../oq-live.b", "oq-live.b.sock", "not-a-socket", ""] {
            assert!(socket_for(dir.path(), Some(bad)).is_err(), "{bad:?}");
        }
        let names: Vec<String> = sockets(dir.path())
            .expect("listed")
            .into_iter()
            .map(|(id, _)| id)
            .collect();
        assert_eq!(names, ["oq-live.a", "oq-live.b"], "sockets only, sorted");
    }

    #[test]
    fn every_trader_is_asked_and_one_that_does_not_answer_is_still_listed() {
        let dir = tempfile::tempdir().expect("dir");
        let listener = UnixListener::bind(dir.path().join("up.sock")).expect("up");
        // A socket nobody accepts on: bound, then the listener dropped.
        drop(UnixListener::bind(dir.path().join("down.sock")).expect("down"));
        let server = std::thread::spawn(move || {
            let (s, _) = listener.accept().expect("accept");
            let mut line = String::new();
            BufReader::new(&s).read_line(&mut line).expect("read");
            let mut w = &s;
            writeln!(w, r#"{{"ok":true,"pid":7}}"#).expect("write");
        });
        let all = ask_each(dir.path(), "status", "test").expect("listed");
        server.join().expect("server");
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].0, "down");
        assert!(all[0].1.is_err());
        assert_eq!(all[1].0, "up");
        assert_eq!(all[1].1.as_ref().expect("answered")["pid"], 7);
    }
}
