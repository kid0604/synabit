//! The ssh executor against a real SSH server, in-process: russh's own server
//! side, listening on 127.0.0.1, recording every command line it is sent.

use super::*;
use crate::syn::connector::manifest::{self, Effect};
use russh::keys::ssh_key::private::Ed25519Keypair;
use russh::server::{self, Auth, Msg, Session};
use russh::{Channel, ChannelId};
use serde_json::json;

pub(crate) const PASSWORD: &str = "raspberry-pw";

/// What `df` prints on the test machine: 21G free on `/`.
pub(crate) const DF: &str = "Filesystem      Size  Used Avail Use% Mounted on\n/dev/root        29G  6.8G   21G  25% /\ntmpfs           1.9G     0  1.9G   0% /dev/shm\n";

fn key(seed: u8) -> PrivateKey {
    PrivateKey::from(Ed25519Keypair::from_seed(&[seed; 32]))
}

/// What the server was sent, and the client key it lets in. A command
/// arrives as `/bin/sh -s` with its line on the input, as from `exec`.
#[derive(Clone)]
struct Pi {
    heard: Arc<Mutex<Vec<String>>>,
    lets_in: PublicKey,
    script: String,
}

impl Pi {
    /// `json` prints rows; `fail` exits 3 with a complaint; anything else is echoed.
    fn answer(&self, channel: ChannelId, line: String, session: &mut Session) -> Result<(), russh::Error> {
        self.heard.lock().unwrap().push(line.clone());
        let (out, err, code) = match line.as_str() {
            "json" => (r#"[{"unit":"nginx","active":true},{"unit":"ssh","active":true}]"#.to_string(), String::new(), 0),
            "fail" => (String::new(), "Unit nope.service could not be found.".to_string(), 3),
            "leak" => (format!("the password is {PASSWORD}"), String::new(), 0),
            // As a Raspberry Pi answers, for the eval.
            l if l.starts_with("df") => (DF.to_string(), String::new(), 0),
            l if l.starts_with("uptime") => (" 09:12:01 up 3 days,  2:41,  1 user,  load average: 0.08, 0.05, 0.01\n".to_string(), String::new(), 0),
            l if l.contains("is-active") => ("active\n".to_string(), String::new(), 0),
            l if l.contains("measure_temp") => ("temp=57.5'C\n".to_string(), String::new(), 0),
            l if l.contains("thermal_zone") => ("57500\n".to_string(), String::new(), 0),
            _ => (line.clone(), String::new(), 0),
        };
        if !out.is_empty() {
            session.data(channel, out.into_bytes())?;
        }
        if !err.is_empty() {
            session.extended_data(channel, 1, err.into_bytes())?;
        }
        session.exit_status_request(channel, code)?;
        session.eof(channel)?;
        session.close(channel)?;
        Ok(())
    }
}

impl server::Handler for Pi {
    type Error = russh::Error;

    async fn auth_password(&mut self, user: &str, password: &str) -> Result<Auth, Self::Error> {
        Ok(if user == "pi" && password == PASSWORD { Auth::Accept } else { Auth::reject() })
    }

    async fn auth_publickey(&mut self, user: &str, key: &PublicKey) -> Result<Auth, Self::Error> {
        Ok(if user == "pi" && key.key_data() == self.lets_in.key_data() { Auth::Accept } else { Auth::reject() })
    }

    async fn channel_open_session(&mut self, _: Channel<Msg>, _: &mut Session) -> Result<bool, Self::Error> {
        Ok(true)
    }

    async fn exec_request(&mut self, channel: ChannelId, data: &[u8], session: &mut Session) -> Result<(), Self::Error> {
        session.channel_success(channel)?;
        let asked = String::from_utf8_lossy(data).into_owned();
        if asked == SHELL {
            self.script.clear();
            return Ok(());
        }
        // Anything else is a login shell asked to run a line itself — what
        // this executor never does.
        self.answer(channel, format!("NOT VIA SH: {asked}"), session)
    }

    async fn data(&mut self, _: ChannelId, data: &[u8], _: &mut Session) -> Result<(), Self::Error> {
        self.script.push_str(&String::from_utf8_lossy(data));
        Ok(())
    }

    async fn channel_eof(&mut self, channel: ChannelId, session: &mut Session) -> Result<(), Self::Error> {
        let script = std::mem::take(&mut self.script);
        // Every command comes after the PATH it is run with.
        let line = match script.strip_prefix(&format!("{PREAMBLE}\n")) {
            Some(line) => line.trim_end_matches('\n').to_string(),
            None => format!("NO PREAMBLE: {script}"),
        };
        self.answer(channel, line, session)
    }
}

/// A server on a free port: its address, its host key's fingerprint, and what it heard.
pub(crate) async fn pi() -> (u16, String, Arc<Mutex<Vec<String>>>) {
    let host = key(1);
    let fp = fingerprint(host.public_key());
    let config = Arc::new(server::Config {
        keys: vec![host],
        auth_rejection_time: Duration::from_millis(10),
        auth_rejection_time_initial: Some(Duration::from_millis(0)),
        inactivity_timeout: Some(Duration::from_secs(30)),
        ..Default::default()
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let heard = Arc::new(Mutex::new(Vec::new()));
    let handler = Pi { heard: heard.clone(), lets_in: key(2).public_key().clone(), script: String::new() };
    tokio::spawn(async move {
        while let Ok((socket, _)) = listener.accept().await {
            let (config, handler) = (config.clone(), handler.clone());
            tokio::spawn(async move {
                if let Ok(s) = server::run_stream(config, socket, handler).await {
                    let _ = s.await;
                }
            });
        }
    });
    (port, fp, heard)
}

fn declaration(port: u16) -> Manifest {
    manifest::parse_yaml(&format!(
        r#"
id: pi
name: Raspberry Pi
transport: ssh
base_url: ssh://pi@127.0.0.1:{port}
auth: {{ type: ssh, secret: login }}
operations:
  - name: echo
    description: Says back what it is given.
    effect: read_attested
    command: [echo, "{{{{text}}}}", "{{{{n}}}}"]
    params:
      text: {{ required: true }}
      n: {{ type: integer }}
  - name: units
    description: Services and whether each runs.
    effect: read_attested
    command: [json]
    result: {{ each: "$" }}
  - name: fail
    description: Fails.
    effect: write
    command: [fail]
  - name: leak
    description: Prints the password.
    effect: read_attested
    command: [leak]
"#
    ))
    .expect("reads")
}

fn instance(pin: Option<&str>, network: Network) -> Instance {
    Instance {
        manifest_id: "pi".into(),
        label: String::new(),
        zone: Default::default(),
        exposure: Default::default(),
        network,
        pinned_cert: pin.map(str::to_string),
        attested: Default::default(),
        mirrors: Default::default(),
        folder: None,
        inputs: Default::default(),
        operations: Default::default(),
        approved: "test".into(),
        approved_at: String::new(),
    }
}

fn op<'a>(m: &'a Manifest, name: &str) -> &'a Operation {
    m.operations.iter().find(|o| o.name == name).expect("declared")
}

#[test]
fn an_address_is_a_user_and_a_host() {
    assert_eq!(target("ssh://pi@raspberrypi.local").unwrap(), Target { user: "pi".into(), host: "raspberrypi.local".into(), port: 22 });
    assert_eq!(target("ssh://admin@192.168.1.50:2222").unwrap().port, 2222);
    assert_eq!(target("ssh://pi@[fe80::1]").unwrap().host, "fe80::1");
    for bad in ["https://pi@host", "ssh://host", "ssh://pi:secret@host", "ssh://pi@host/var", "ssh://pi@host?x=1", "pi@host"] {
        assert!(target(bad).is_err(), "{bad}");
    }
}

/// What the line is to a real POSIX shell, run here: each value comes back
/// as the one word it was, whatever it holds. The same shell the machine on
/// the other side runs the line with (`SHELL`).
#[cfg(unix)]
#[test]
fn a_real_shell_reads_each_value_as_one_word() {
    use std::io::Write;
    let mut o = op(&declaration(22), "echo").clone();
    o.command = vec!["printf".into(), "[%s]\\n".into(), "{{v}}".into()];
    o.params = [("v".to_string(), manifest::Param { ty: "string".into(), description: String::new(), required: true, choices: vec![], role: None })].into();
    for value in ["x; touch /tmp/pwned", "$(id)", "`id`", "it's", r"\'; id #", "a\nb", "*", "~", "$HOME", "\\", "' ; id ; '"] {
        let line = command_line(&o, &json!({ "v": value })).unwrap();
        let mut sh = std::process::Command::new("/bin/sh")
            .arg("-s")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        sh.stdin.take().unwrap().write_all(format!("{line}\n").as_bytes()).unwrap();
        let out = String::from_utf8(sh.wait_with_output().unwrap().stdout).unwrap();
        assert_eq!(out, format!("[{value}]\n"), "{value:?} as {line}");
    }
}

/// A value is one word, whatever it holds.
#[test]
fn a_value_is_one_word_whatever_it_holds() {
    let m = declaration(22);
    let echo = op(&m, "echo");
    for (value, sent) in [
        ("hello", "echo hello"),
        ("two words", "echo 'two words'"),
        ("x; rm -rf ~", "echo 'x; rm -rf ~'"),
        ("$(reboot)", "echo '$(reboot)'"),
        ("`id`", "echo '`id`'"),
        ("it's", r"echo 'it'\''s'"),
        ("a\nb", "echo 'a\nb'"),
        ("", "echo ''"),
    ] {
        assert_eq!(command_line(echo, &json!({ "text": value })).unwrap(), sent, "{value:?}");
    }
    assert_eq!(command_line(echo, &json!({ "text": "x", "n": 5 })).unwrap(), "echo x 5");
}

/// An option is the declaration's to write.
#[test]
fn a_filled_word_cannot_be_an_option() {
    let m = declaration(22);
    let echo = op(&m, "echo");
    assert!(command_line(echo, &json!({ "text": "--help" })).is_err());
    assert!(command_line(echo, &json!({ "text": "-e" })).is_err());
    assert!(command_line(echo, &json!({})).is_err(), "required");
    assert!(command_line(echo, &json!({ "text": { "a": 1 } })).is_err(), "not a plain value");
    assert!(command_line(echo, &json!({ "text": "x", "n": "-1" })).is_err(), "a value is never an option, even after one");
}

#[test]
fn an_optional_word_is_left_out_and_a_list_is_one_word_each() {
    let mut o = op(&declaration(22), "echo").clone();
    o.command = vec!["systemctl".into(), "status".into(), "{{units}}".into()];
    o.params = [("units".to_string(), manifest::Param { ty: "array".into(), description: String::new(), required: false, choices: vec![], role: None })].into();
    assert_eq!(command_line(&o, &json!({})).unwrap(), "systemctl status");
    assert_eq!(command_line(&o, &json!({ "units": ["nginx", "my app"] })).unwrap(), "systemctl status nginx 'my app'");
    assert!(command_line(&o, &json!({ "units": ["nginx", "--all"] })).is_err());
}

/// Nothing a model fills can choose the program, or hand code to an interpreter.
#[test]
fn the_program_is_never_filled() {
    let words = |w: &[&str]| w.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    for bad in [
        words(&["{{program}}", "x"]),
        words(&["bash", "-c", "{{script}}"]),
        words(&["/usr/bin/python3", "-c", "print({{x}})"]),
        words(&["awk", "{{program}}", "/var/log/syslog"]),
        words(&["sudo", "{{program}}"]),
        words(&["sudo", "-u", "www", "{{program}}"]),
        words(&["env", "{{program}}"]),
        words(&["xargs", "{{program}}"]),
        words(&["timeout", "10", "{{program}}"]),
        words(&["sudo", "-u", "{{who}}", "id"]),
        words(&["ssh", "other-host", "{{command}}"]),
        words(&["watch", "{{command}}"]),
        // Found in review (2026-10-13): each a way to run what the model wrote.
        words(&["env", "LANG=C", "{{program}}"]),
        words(&["sudo", "VAR=1", "{{program}}"]),
        words(&["find", "/x", "-name", "y", "-exec", "{{cmd}}", "{}", ";"]),
        words(&["tar", "--to-command={{x}}", "-xf", "a.tar"]),
        words(&["rsync", "-e{{x}}", "a", "b"]),
        words(&["git", "-c", "{{setting}}", "status"]),
        words(&["sed", "{{expr}}", "file"]),
        words(&["sqlite3", "db.sqlite", "{{sql}}"]),
        words(&["docker", "exec", "app", "{{cmd}}"]),
        words(&["python3.11", "-c", "{{code}}"]),
        words(&["/usr/bin/perl5.36", "{{script}}"]),
        words(&["echo", "a\nb"]),
        vec![],
    ] {
        assert!(!command_problems(&bad).is_empty(), "{bad:?}");
    }
    for good in [
        words(&["sudo", "systemctl", "restart", "{{unit}}"]),
        words(&["sudo", "-n", "systemctl", "restart", "{{unit}}"]),
        words(&["sudo", "-u", "www", "systemctl", "restart", "{{unit}}"]),
        words(&["timeout", "10", "ping", "-c", "1", "{{host}}"]),
        words(&["env", "LANG=C", "df", "-h", "{{path}}"]),
        words(&["find", "/var/log", "-name", "{{pattern}}"]),
        words(&["df", "-h", "/"]),
        words(&["journalctl", "-u", "{{unit}}", "-n", "{{lines}}"]),
        words(&["bash", "/home/pi/backup.sh"]),
    ] {
        assert!(command_problems(&good).is_empty(), "{good:?}");
    }
}

#[test]
fn a_declaration_says_what_is_wrong_with_it() {
    let codes = |yaml: &str| manifest::problems(&manifest::parse_yaml(yaml).expect("reads")).into_iter().map(|p| p.code).collect::<Vec<_>>();
    let base = "id: pi\nname: Pi\ntransport: ssh\nbase_url: ssh://pi@raspberrypi.local\nauth: { type: ssh, secret: login }\n";
    assert!(codes(&format!("{base}operations:\n  - {{ name: disk, description: Disk use., effect: read_attested, command: [df, -h] }}\n")).is_empty());
    assert!(codes(&format!("{base}operations:\n  - {{ name: disk, description: Disk use., effect: read, command: [df, -h] }}\n")).contains(&"effect_above_ceiling"), "nothing proves a command reads");
    assert!(codes(&format!("{base}operations:\n  - {{ name: run, description: Anything., effect: write, command: [sh, -c, \"{{{{c}}}}\"], params: {{ c: {{}} }} }}\n")).contains(&"bad_command"));
    assert!(codes(&format!("{base}operations:\n  - {{ name: get, description: A call., effect: read, call: {{ method: GET, path: /x }} }}\n")).contains(&"ssh_has_call"));
    assert!(codes(&format!("{base}operations:\n  - {{ name: d, description: D., effect: write, command: [df, \"{{{{x}}}}\"] }}\n")).contains(&"unknown_param"));
    let no_login = "id: pi\nname: Pi\ntransport: ssh\nbase_url: ssh://pi@raspberrypi.local\noperations:\n  - { name: disk, description: D., effect: write, command: [df] }\n";
    assert!(codes(no_login).contains(&"ssh_auth"));
    let no_user = "id: pi\nname: Pi\ntransport: ssh\nbase_url: ssh://raspberrypi.local\nauth: { type: ssh, secret: login }\noperations:\n  - { name: disk, description: D., effect: write, command: [df] }\n";
    assert!(codes(no_user).contains(&"bad_url"));
    let on_http = "id: x\nname: X\nbase_url: https://x.example\noperations:\n  - { name: d, description: D., effect: write, call: { method: POST, path: /d }, command: [df] }\n";
    assert!(codes(on_http).contains(&"command_on_http"));
}

/// A key pasted into a one-line password field arrives without its line
/// breaks, and is read anyway.
#[test]
fn a_key_is_read_however_it_was_pasted() {
    let pem = key(2).to_openssh(russh::keys::ssh_key::LineEnding::LF).unwrap().to_string();
    for pasted in [pem.clone(), pem.replace('\n', " "), pem.replace('\n', "")] {
        assert!(matches!(login(&pasted), Ok(Login::Key(_))), "{pasted:.40}");
    }
    assert!(matches!(login("raspberry"), Ok(Login::Password(p)) if p == "raspberry"));
    assert!(login("-----BEGIN OPENSSH PRIVATE KEY----- AAAA").is_err(), "cut short");
}

/// The first meeting stops at the host key and says what it is; the pinned
/// key gets in; a different one is refused.
#[tokio::test]
async fn the_machine_is_the_one_the_person_met() {
    let (port, fp, heard) = pi().await;
    let m = declaration(port);

    let first = check(&m, &instance(None, Network::Internal), Some(PASSWORD)).await.err().expect("not met yet");
    assert_eq!(first.failed.code, Some("host_key_unknown"));
    assert_eq!(first.host_key.as_deref(), Some(fp.as_str()));
    assert!(fp.starts_with("SHA256:"));

    assert!(check(&m, &instance(Some(&fp), Network::Internal), Some(PASSWORD)).await.is_ok());

    let other = fingerprint(key(9).public_key());
    let changed = check(&m, &instance(Some(&other), Network::Internal), Some(PASSWORD)).await.err().expect("a stranger");
    assert_eq!(changed.failed.code, Some("host_key_changed"));
    assert_eq!(changed.host_key.as_deref(), Some(fp.as_str()));
    assert!(heard.lock().unwrap().is_empty(), "nothing was run while checking");
}

#[tokio::test]
async fn a_wrong_login_is_said_so_and_a_key_logs_in() {
    let (port, fp, _) = pi().await;
    let m = declaration(port);
    let inst = instance(Some(&fp), Network::Internal);
    let wrong = check(&m, &inst, Some("not-it")).await.err().expect("refused");
    assert_eq!(wrong.failed.code, Some("credential_rejected"));
    assert!(check(&m, &inst, None).await.is_err(), "no login at all");

    let pem = key(2).to_openssh(russh::keys::ssh_key::LineEnding::LF).unwrap().to_string();
    assert!(check(&m, &inst, Some(&pem)).await.is_ok(), "the key it lets in");
    let stranger = key(3).to_openssh(russh::keys::ssh_key::LineEnding::LF).unwrap().to_string();
    assert_eq!(check(&m, &inst, Some(&stranger)).await.err().expect("refused").failed.code, Some("credential_rejected"));
}

/// What is sent is the quoted line, exactly; what comes back is a table.
#[tokio::test]
async fn a_command_runs_and_comes_back_as_a_table() {
    let (port, fp, heard) = pi().await;
    let m = declaration(port);
    let inst = instance(Some(&fp), Network::Internal);

    let echoed = run(&m, &inst, op(&m, "echo"), &json!({ "text": "x; reboot" }), Some(PASSWORD)).await.expect("ran");
    assert_eq!(heard.lock().unwrap().last().map(String::as_str), Some("echo 'x; reboot'"));
    assert_eq!(echoed.rows, vec![vec![json!("echo 'x; reboot'")]], "text output is one row with `text`");
    assert!(echoed.asked.contains("pi@127.0.0.1"), "{}", echoed.asked);
    assert!(!echoed.asked.contains(PASSWORD));

    let units = run(&m, &inst, op(&m, "units"), &json!({}), Some(PASSWORD)).await.expect("ran");
    assert_eq!(units.rows.len(), 2, "JSON output is read as records");
    assert!(units.columns.iter().any(|c| c.name == "unit"));
}

#[tokio::test]
async fn a_failing_command_says_how_and_the_login_is_never_echoed() {
    let (port, fp, _) = pi().await;
    let m = declaration(port);
    let inst = instance(Some(&fp), Network::Internal);

    let failed = run(&m, &inst, op(&m, "fail"), &json!({}), Some(PASSWORD)).await.err().expect("exit 3");
    assert_eq!(failed.code, Some("command_failed"));
    assert!(failed.answered);
    assert!(failed.said.contains("exited with 3") && failed.said.contains("could not be found"), "{}", failed.said);

    let leaked = run(&m, &inst, op(&m, "leak"), &json!({}), Some(PASSWORD)).await.expect("ran");
    let text = leaked.rows[0][0].as_str().unwrap_or_default();
    assert!(!text.contains(PASSWORD) && text.contains("[hidden]"), "{text}");
}

/// A host approved as public that turns out to be this machine is not reached.
#[tokio::test]
async fn a_public_host_stays_public() {
    let (port, fp, heard) = pi().await;
    let m = declaration(port);
    let refused = run(&m, &instance(Some(&fp), Network::Public), op(&m, "units"), &json!({}), Some(PASSWORD)).await.err().expect("refused");
    assert!(refused.said.contains("private network"), "{}", refused.said);
    assert!(heard.lock().unwrap().is_empty());
}

/// The effect a command may be treated as: never a proven read.
#[test]
fn a_command_is_a_send_until_vouched_for() {
    let m = declaration(22);
    let inst = instance(None, Network::Internal);
    assert_eq!(crate::syn::connector::instance::effect_here(&inst, op(&m, "units")), Effect::Write);
}

/// A model writes the command as one string and `{x}`; it reads as words.
#[test]
fn a_draft_written_loosely_reads_as_a_command() {
    let yaml = r#"
id: pi
name: Raspberry Pi
transport: ssh
base_url: "{{input.host}}"
auth: { type: ssh, secret: login }
inputs:
  - { name: host, kind: address, label: Address, example: "ssh://pi@raspberrypi.local" }
  - { name: login, kind: secret, label: Password }
operations:
  - name: restart
    description: Restart a service.
    command: "sudo systemctl restart {unit}"
  - name: disk
    description: Disk use.
    effect: read_attested
    command: [df, -h, /]
"#;
    let Ok(crate::syn::connector::authoring::Written::Declaration(m)) = crate::syn::connector::authoring::read_written(yaml, &json!({})) else {
        panic!("reads")
    };
    let restart = op(&m, "restart");
    assert_eq!(restart.command, vec!["sudo", "systemctl", "restart", "{{unit}}"]);
    assert_eq!(restart.effect, Effect::Write, "a send unless said otherwise");
    assert!(restart.params.get("unit").is_some_and(|p| p.required));
    assert!(restart.call.is_empty());
    let problems = crate::syn::connector::authoring::check(&m, &[], None);
    assert!(problems.is_empty(), "{problems:?}");
}

/// The address the person types is approved only as an ssh address, and a
/// home or Tailscale address is their own network.
#[test]
fn the_typed_address_is_an_ssh_address_on_their_network() {
    use crate::syn::connector::instance::{self as inst, network_for, Choices};
    let dir = tempfile::tempdir().unwrap();
    let vault = dir.path().to_str().unwrap();
    let m = manifest::parse_yaml(
        "id: pi\nname: Pi\ntransport: ssh\nbase_url: \"{{input.host}}\"\nauth: { type: ssh, secret: login }\ninputs:\n  - { name: host, kind: address, label: Address }\n  - { name: login, kind: secret, label: Password }\noperations:\n  - { name: disk, description: Disk use., effect: read_attested, command: [df, -h] }\n",
    )
    .unwrap();
    let with = |host: &str| Choices { inputs: [("host".to_string(), host.to_string())].into(), ..Default::default() };
    assert!(inst::approve(vault, &m, with("https://raspberrypi.local"), "now").is_err());
    assert!(inst::approve(vault, &m, with("ssh://raspberrypi.local"), "now").is_err(), "no user");
    assert!(inst::approve(vault, &m, with("ssh://pi@raspberrypi.local"), "now").is_ok());

    for home in ["ssh://pi@raspberrypi.local", "ssh://pi@192.168.1.50", "ssh://pi@100.101.102.103", "ssh://pi@pi.tail1234.ts.net", "ssh://pi@nas"] {
        assert_eq!(network_for(home), Network::Internal, "{home}");
    }
    assert_eq!(network_for("ssh://pi@203.0.113.7"), Network::Public);
}

/// What only looks runs as a read; anything that could change the machine,
/// never end, or be more than words to a shell is a send.
#[test]
fn a_line_that_only_looks_is_told_from_one_that_does() {
    for line in [
        "uptime",
        "df -h /",
        "free -m",
        "ps aux",
        "top -bn1",
        "top -b -n 1",
        "hostname",
        "hostname -I",
        "vcgencmd measure_temp",
        "systemctl status openclaw-gateway",
        "systemctl --failed",
        "journalctl -u ssh -n 50 --no-pager",
        "ip addr",
    ] {
        assert!(reads_only(line), "{line}");
    }
    for line in [
        "htop",
        "top",
        "top -n 1",
        "hostname pwned",
        "systemctl stop openclaw-gateway",
        "systemctl restart ssh",
        "journalctl -f",
        "journalctl --vacuum-time=1d",
        "ip link set eth0 down",
        "ps aux | grep openclaw",
        "df -h; reboot",
        "ls $(rm -rf ~)",
        "ls `id`",
        "ls > /etc/passwd",
        "kill 1045",
        "pkill -f openclaw-gateway",
        "sudo df -h",
        "openclaw gateway status",
        "",
    ] {
        assert!(!reads_only(line), "{line}");
    }
}

#[test]
fn a_run_line_is_one_line_of_reasonable_length() {
    assert_eq!(run_line(&json!({ "command": "  df -h /  " })).as_deref(), Ok("df -h /"));
    assert!(run_line(&json!({})).is_err());
    assert!(run_line(&json!({ "command": "   " })).is_err());
    assert!(run_line(&json!({ "command": "x".repeat(4_001) })).is_err());
    assert!(run_line(&json!({ "command": "echo a\u{0}b" })).is_err());
}
