//! Declarations that come with the app.
//!
//! A bundled connector is a declaration like any other — `provenance:
//! bundled` — written here rather than by the person or by Syn. Adding one
//! files it in the vault; it is not usable until approved on the device, the
//! same as one pasted in. They are also the examples Syn learns the format from.

/// One declaration the app offers.
pub struct Bundled {
    pub id: &'static str,
    pub name: &'static str,
    /// The prose above the declaration in the note.
    pub about: &'static str,
    pub yaml: &'static str,
}

/// Historical daily weather for any place, from Open-Meteo: no account, no key.
/// Chosen as the first because it answers a personal question — "how much did
/// it rain this month, compared with last" — over enough rows to need a table.
pub const WEATHER: Bundled = Bundled {
    id: "weather",
    name: "Weather (Open-Meteo)",
    about: "Past weather, day by day, anywhere — thời tiết từng ngày đã qua — from Open-Meteo. No account, no key.",
    yaml: r#"id: weather
name: Weather (Open-Meteo)
provenance: bundled
base_url: https://archive-api.open-meteo.com/v1
operations:
  - name: daily
    description: "Weather of each day in a past date range at one place: rain (mm), highest and lowest temperature (°C), wind (km/h). Give the place as latitude and longitude — e.g. Hà Nội 21.03, 105.85; Hồ Chí Minh 10.82, 106.63; Đà Nẵng 16.05, 108.21. Ask for the whole range in one call: two months at once, not one call each."
    effect: read
    call:
      method: GET
      path: /archive
      query:
        latitude: "{{latitude}}"
        longitude: "{{longitude}}"
        start_date: "{{start_date}}"
        end_date: "{{end_date}}"
        daily: precipitation_sum,temperature_2m_max,temperature_2m_min,wind_speed_10m_max
        timezone: "{{timezone}}"
    params:
      latitude: { type: number, required: true, description: "Latitude, e.g. 21.03" }
      longitude: { type: number, required: true, description: "Longitude, e.g. 105.85" }
      start_date: { required: true, description: "YYYY-MM-DD" }
      end_date: { required: true, description: "YYYY-MM-DD, not after a few days ago" }
      timezone: { description: "e.g. Asia/Ho_Chi_Minh. Defaults to the place's own." }
    result:
      zip: $.daily
      pick:
        day: { path: time, type: time }
        rain_mm: { path: precipitation_sum, type: number }
        temp_max_c: { path: temperature_2m_max, type: number }
        temp_min_c: { path: temperature_2m_min, type: number }
        wind_max_kmh: { path: wind_speed_10m_max, type: number }
"#,
};

/// Exchange rates of the European Central Bank, by day. No account.
pub const RATES: Bundled = Bundled {
    id: "rates",
    name: "Exchange rates (Frankfurter)",
    about: "Exchange rates by day — tỷ giá hối đoái — from the European Central Bank, through Frankfurter. No account, no key.",
    yaml: r#"id: rates
name: Exchange rates (Frankfurter)
provenance: bundled
base_url: https://api.frankfurter.dev/v1
operations:
  - name: on_day
    description: "The rate of one currency in others on a day (YYYY-MM-DD), or the latest with day \"latest\". Rates are published on working days."
    effect: read
    call: { method: GET, path: "/{{day}}", query: { base: "{{from}}", symbols: "{{to}}" } }
    params:
      day: { required: true, description: "YYYY-MM-DD or latest" }
      from: { required: true, description: "Currency code, e.g. EUR" }
      to: { description: "Currency codes, comma-separated, e.g. USD,JPY" }
    result:
      pick:
        day: { path: date, type: time }
        base: base
        rates: { path: rates, type: json }
"#,
};

/// Public holidays by country and year, from Nager.Date. No account.
pub const HOLIDAYS: Bundled = Bundled {
    id: "holidays",
    name: "Public holidays (Nager.Date)",
    about: "Public holidays of a country in a year — ngày nghỉ lễ — from Nager.Date. No account, no key.",
    yaml: r#"id: holidays
name: Public holidays (Nager.Date)
provenance: bundled
base_url: https://date.nager.at/api/v3
operations:
  - name: in_year
    description: "Public holidays of one country in one year, with their local and English names."
    effect: read
    call: { method: GET, path: "/PublicHolidays/{{year}}/{{country}}" }
    params:
      year: { type: integer, required: true, description: "e.g. 2026" }
      country: { required: true, description: "Two-letter code, e.g. VN, US" }
    result:
      pick:
        day: { path: date, type: time }
        local_name: localName
        name: name
"#,
};

/// Any CalDAV calendar: Fastmail, Nextcloud, Radicale, a NAS.
pub const CALDAV: Bundled = Bundled {
    id: "calendar",
    name: "Calendar (CalDAV)",
    about: "A calendar on Fastmail, Nextcloud, a NAS or any CalDAV server — lịch — read by its address.",
    yaml: r#"id: calendar
name: Calendar (CalDAV)
provenance: bundled
decoder: ics
inputs:
  - name: server
    kind: address
    label: { en: Calendar address, vi: Địa chỉ lịch }
    example: https://cloud.example.com/remote.php/dav/calendars/me/personal/
    guide:
      steps:
        en:
          - "Fastmail: Settings → Calendars → the calendar → Export or share → copy the CalDAV URL."
          - "Nextcloud: Calendar → ⋯ beside the calendar → Copy private link."
          - "A NAS or another server: its calendar's CalDAV address, ending with /."
        vi:
          - "Fastmail: Cài đặt → Lịch → chọn lịch → Xuất hoặc chia sẻ → chép địa chỉ CalDAV."
          - "Nextcloud: Lịch → ⋯ cạnh lịch → Sao chép liên kết riêng."
          - "NAS hoặc máy chủ khác: địa chỉ CalDAV của lịch, kết thúc bằng /."
  - name: user
    kind: account
    label: { en: User name, vi: Tên đăng nhập }
    example: me@fastmail.com
  - name: app_password
    kind: secret
    label: { en: App password, vi: Mật khẩu ứng dụng }
    guide:
      steps:
        en:
          - "Fastmail: Settings → Privacy & Security → App passwords → New, with access to Calendars."
          - "Nextcloud: Settings → Security → Create new app password."
          - "Use an app password, not your account's own password."
        vi:
          - "Fastmail: Cài đặt → Quyền riêng tư và bảo mật → Mật khẩu ứng dụng → Tạo mới, cho phép Lịch."
          - "Nextcloud: Cài đặt → Bảo mật → Tạo mật khẩu ứng dụng mới."
          - "Dùng mật khẩu ứng dụng, đừng dùng mật khẩu tài khoản."
base_url: "{{input.server}}"
auth: { type: basic, user: "{{input.user}}", secret: app_password }
operations:
  - name: check
    description: Whether the calendar answers.
    effect: read
    role: discover
    decoder: xml
    call: { method: PROPFIND, path: /, headers: { Depth: "0" } }
  - name: events
    description: "Events between two moments, in UTC like 20261012T000000Z. The day after the last day is the end."
    effect: read
    call:
      method: REPORT
      path: /
      headers: { Depth: "1" }
      body: '<c:calendar-query xmlns:d="DAV:" xmlns:c="urn:ietf:params:xml:ns:caldav"><d:prop><d:getetag/><c:calendar-data/></d:prop><c:filter><c:comp-filter name="VCALENDAR"><c:comp-filter name="VEVENT"><c:time-range start="{{start}}" end="{{end}}"/></c:comp-filter></c:comp-filter></c:filter></c:calendar-query>'
    params:
      start: { required: true, description: "UTC, e.g. 20261012T000000Z" }
      end: { required: true, description: "UTC, e.g. 20261019T000000Z" }
mirrors:
  - name: keep
    from: events
    args: { start: "{{window_start}}", end: "{{window_end}}" }
    window: { past_days: 7, future_days: 90 }
    every_minutes: 30
    key: uid
    into: event
    fields: { title: summary, content: description, start_at: dtstart, end_at: dtend, location: location, tzid: dtstart_tz, rrule: rrule }
"#,
};

/// Any CardDAV address book.
pub const CARDDAV: Bundled = Bundled {
    id: "contacts",
    name: "Contacts (CardDAV)",
    about: "An address book on Fastmail, Nextcloud, a NAS or any CardDAV server — danh bạ — read by its address.",
    yaml: r#"id: contacts
name: Contacts (CardDAV)
provenance: bundled
decoder: vcard
inputs:
  - name: server
    kind: address
    label: { en: Address book address, vi: Địa chỉ danh bạ }
    example: https://cloud.example.com/remote.php/dav/addressbooks/users/me/contacts/
    guide:
      steps:
        en:
          - "Fastmail: Settings → Contacts → the address book → copy the CardDAV URL."
          - "Nextcloud: Contacts → Settings → ⋯ beside the address book → Copy link."
        vi:
          - "Fastmail: Cài đặt → Danh bạ → chọn danh bạ → chép địa chỉ CardDAV."
          - "Nextcloud: Danh bạ → Cài đặt → ⋯ cạnh danh bạ → Sao chép liên kết."
  - name: user
    kind: account
    label: { en: User name, vi: Tên đăng nhập }
  - name: app_password
    kind: secret
    label: { en: App password, vi: Mật khẩu ứng dụng }
    guide:
      steps:
        en: ["Create an app password in your account's security settings, with access to Contacts."]
        vi: ["Tạo một mật khẩu ứng dụng trong phần bảo mật của tài khoản, cho phép Danh bạ."]
base_url: "{{input.server}}"
auth: { type: basic, user: "{{input.user}}", secret: app_password }
operations:
  - name: check
    description: Whether the address book answers.
    effect: read
    role: discover
    decoder: xml
    call: { method: PROPFIND, path: /, headers: { Depth: "0" } }
  - name: search
    description: "Contacts whose name contains a text."
    effect: read
    call:
      method: REPORT
      path: /
      headers: { Depth: "1" }
      body: '<c:addressbook-query xmlns:d="DAV:" xmlns:c="urn:ietf:params:xml:ns:carddav"><d:prop><d:getetag/><c:address-data/></d:prop><c:filter><c:prop-filter name="FN"><c:text-match collation="i;unicode-casemap" match-type="contains">{{name}}</c:text-match></c:prop-filter></c:filter></c:addressbook-query>'
    params:
      name: { required: true, description: "Part of the name" }
"#,
};

/// Bank statements saved in a folder on this computer.
pub const STATEMENTS: Bundled = Bundled {
    id: "statements",
    name: "Bank statements in a folder",
    about: "CSV statements saved in a folder you choose, such as Downloads — sao kê ngân hàng — read on this computer only.",
    yaml: r#"id: statements
name: Bank statements in a folder
provenance: bundled
transport: device
operations:
  - name: rows
    description: "Rows of the CSV statements in the chosen folder, newest file first, each row with its file."
    effect: read
    bridge: folder.watch
    filter: { extensions: [csv] }
    params:
      since_days: { type: integer, description: "How far back, in days (default 31)" }
"#,
};

pub fn all() -> Vec<&'static Bundled> {
    vec![&WEATHER, &RATES, &HOLIDAYS, &CALDAV, &CARDDAV, &STATEMENTS]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_bundled_declaration_reads_and_has_no_problems() {
        for b in all() {
            let m = super::super::manifest::parse_yaml(b.yaml).unwrap_or_else(|p| panic!("{}: {p:?}", b.id));
            assert_eq!(m.id, b.id);
            assert_eq!(m.name, b.name);
            assert_eq!(super::super::manifest::problems(&m), vec![], "{}", b.id);
        }
    }

    /// Against the real Frankfurter and Nager.Date. Needs the network.
    #[tokio::test]
    #[ignore = "reaches frankfurter.dev and date.nager.at; run by hand"]
    async fn the_rates_and_holidays_declarations_work_against_their_services() {
        let inst = |m: &super::super::manifest::Manifest| super::super::instance::Instance {
            manifest_id: m.id.clone(),
            label: String::new(),
            zone: Default::default(),
            exposure: Default::default(),
            network: super::super::instance::Network::Public,
            pinned_cert: None,
            attested: Default::default(),
            mirrors: Default::default(),
            folder: None,
            inputs: Default::default(),
            approved: String::new(),
            approved_at: String::new(),
        };
        let rates = super::super::manifest::parse_yaml(RATES.yaml).expect("reads");
        let got = super::super::http::fetch(&rates, &inst(&rates), &rates.operations[0], &serde_json::json!({ "day": "2026-09-30", "from": "EUR", "to": "USD" }), None).await.expect("answered");
        assert_eq!(got.rows.len(), 1);
        assert!(format!("{:?}", got.rows[0]).contains("1.1355"), "{:?}", got.rows);
        let holidays = super::super::manifest::parse_yaml(HOLIDAYS.yaml).expect("reads");
        let got = super::super::http::fetch(&holidays, &inst(&holidays), &holidays.operations[0], &serde_json::json!({ "year": 2026, "country": "VN" }), None).await.expect("answered");
        assert!(got.rows.len() >= 4 && format!("{:?}", got.rows).contains("2026-09-02"), "{:?}", got.rows);
    }

    /// Against the real Open-Meteo: the declaration asks what it means to ask
    /// and the answer becomes the table it means. Needs the network.
    #[tokio::test]
    #[ignore = "reaches open-meteo.com; run by hand"]
    async fn the_weather_declaration_works_against_open_meteo() {
        let m = super::super::manifest::parse_yaml(WEATHER.yaml).expect("reads");
        let inst = super::super::instance::Instance {
            manifest_id: m.id.clone(),
            label: String::new(),
            zone: Default::default(),
            exposure: Default::default(),
            network: super::super::instance::Network::Public,
            pinned_cert: None,
            attested: Default::default(),
            mirrors: Default::default(),
            folder: None,
            inputs: Default::default(),
            approved: String::new(),
            approved_at: String::new(),
        };
        let args = serde_json::json!({
            "latitude": 21.03, "longitude": 105.85,
            "start_date": "2026-09-01", "end_date": "2026-09-30", "timezone": "Asia/Ho_Chi_Minh"
        });
        let got = super::super::http::run(&m, &inst, &m.operations[0], &args, None).await.expect("answered");
        assert_eq!(got.rows.len(), 30, "{:?}", got.missing);
        let names: Vec<&str> = got.columns.iter().map(|c| c.name.as_str()).collect();
        assert!(names.contains(&"rain_mm") && names.contains(&"day"), "{names:?}");
        assert!(got.asked.starts_with("GET https://archive-api.open-meteo.com/v1/archive?"), "{}", got.asked);
        assert!(got.asked.contains("latitude=21.03") && got.asked.contains("start_date=2026-09-01"), "{}", got.asked);
        eprintln!("{} rows; first {:?}", got.rows.len(), got.rows.first());
    }
}
