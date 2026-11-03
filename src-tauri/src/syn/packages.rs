//! A list of packages to install, as `machine_env` is given it.
//!
//! The list is the only thing the model chooses about an environment, and it
//! goes onto the card the person approves and then to the installer. So it is
//! held to the shape of a requirement — a name, optional extras, an optional
//! version — and nothing else: no URL, no path, no `--index-url`, no `-e`, no
//! second line. Anything an installer would read as an instruction rather than
//! a package is refused here, before anybody is asked.
//!
//! Not under `syn::machine`, which is macOS only: the gate and the engine ask
//! this on every platform, to put the list on the card.

use serde_json::Value;

/// At most this many packages in one environment the model asks for.
pub const MOST: usize = 40;

/// The Python every environment is made with.
pub const PYTHON: &str = "3.12";

fn name_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-')
}

/// One requirement, checked: `numpy`, `numpy==2.5.4`, `python-docx>=1.1`,
/// `uvicorn[standard]~=0.30`.
pub fn check(spec: &str) -> Result<String, String> {
    let spec = spec.trim();
    let refuse = || Err(format!("\"{}\" is not a package name with an optional version, like numpy or numpy==2.5.4.", spec.chars().take(80).collect::<String>()));
    let name_end = spec.find(|c: char| !name_char(c)).unwrap_or(spec.len());
    let (name, mut rest) = spec.split_at(name_end);
    if name.is_empty() || !name.starts_with(|c: char| c.is_ascii_alphanumeric()) || name.len() > 100 {
        return refuse();
    }
    if let Some(extras) = rest.strip_prefix('[') {
        let Some((inside, after)) = extras.split_once(']') else { return refuse() };
        if inside.is_empty() || !inside.split(',').all(|e| !e.is_empty() && e.chars().all(name_char)) {
            return refuse();
        }
        rest = after;
    }
    if !rest.is_empty() {
        let op = ["==", ">=", "<=", "~=", "!=", ">", "<"].into_iter().find(|op| rest.starts_with(op));
        let Some(op) = op else { return refuse() };
        let version = &rest[op.len()..];
        if version.is_empty() || version.len() > 40 || !version.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '*' | '+' | '!')) {
            return refuse();
        }
    }
    Ok(spec.to_string())
}

/// The whole list, checked, from a call's arguments.
pub fn parse(args: &Value) -> Result<Vec<String>, String> {
    let Some(given) = args.get("packages").and_then(Value::as_array).filter(|p| !p.is_empty()) else {
        return Err("`packages` is a list of package names, like [\"numpy\", \"python-docx\"].".into());
    };
    if given.len() > MOST {
        return Err(format!("At most {MOST} packages in one environment."));
    }
    let mut out = Vec::new();
    for one in given {
        let Some(spec) = one.as_str() else { return Err("Each package is a string.".into()) };
        let spec = check(spec)?;
        if !out.contains(&spec) {
            out.push(spec);
        }
    }
    Ok(out)
}

/// What the card says, in Rust's words: the packages, one a line, and where
/// they come from. `None` when the list would be refused anyway.
pub fn detail(args: &Value) -> Option<String> {
    let packages = parse(args).ok()?;
    Some(format!("From PyPI, with Python {PYTHON}:\n{}", packages.join("\n")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_requirement_is_a_name_and_a_version_and_nothing_else() {
        for ok in ["numpy", "numpy==2.5.4", "python-docx>=1.1", "uvicorn[standard]~=0.30", "zope.interface", "pandas<3", "x==1.0.*"] {
            assert_eq!(check(ok).as_deref(), Ok(ok), "{ok}");
        }
        for bad in [
            "", "-e .", "--index-url https://evil.example/simple", "./evil", "/tmp/x.whl", "https://evil.example/x.whl",
            "git+https://github.com/x/y", "numpy @ https://evil.example/n.whl", "numpy\n--extra-index-url x", "numpy; os_name=='posix'",
            "numpy==", "numpy=2", "[x]", "a[]", "numpy==1.0 extra", "_private",
        ] {
            assert!(check(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn the_card_shows_the_list_as_it_will_be_installed() {
        let args = serde_json::json!({ "packages": ["numpy==2.5.4", "python-docx", "numpy==2.5.4"] });
        assert_eq!(parse(&args).unwrap(), ["numpy==2.5.4", "python-docx"]);
        assert_eq!(detail(&args).unwrap(), "From PyPI, with Python 3.12:\nnumpy==2.5.4\npython-docx");
        assert!(detail(&serde_json::json!({ "packages": ["--index-url x"] })).is_none());
        let many: Vec<String> = (0..=MOST).map(|i| format!("p{i}")).collect();
        assert!(parse(&serde_json::json!({ "packages": many })).is_err());
    }
}
