//! What the bot says in its own voice, rather than Syn's.
//!
//! Syn answers in whatever language it is written to. These are the handful of
//! fixed lines around it — a pairing, a stop, a status — and they follow the
//! computer's language, Vietnamese or English, because Rust has no way to ask
//! the front end which one it chose.

use chrono::NaiveDateTime;

use crate::calendar::reminders::PlannedReminder;

pub struct Words {
    vietnamese: bool,
}

/// One `capture` that kept something, as the confirmation reports it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Kept {
    /// The start of the words kept; empty when only files were.
    pub text: String,
    pub images: usize,
    pub audio: usize,
    pub files: usize,
}

impl Words {
    /// In the language this computer is set to.
    pub fn here() -> Self {
        let vietnamese = tauri_plugin_os::locale().is_some_and(|l| l.to_lowercase().starts_with("vi"));
        Self { vietnamese }
    }

    #[cfg(test)]
    pub fn english() -> Self {
        Self { vietnamese: false }
    }

    fn pick(&self, vi: &'static str, en: &'static str) -> &'static str {
        if self.vietnamese {
            vi
        } else {
            en
        }
    }

    /// The menu Telegram shows when somebody types `/`.
    pub fn commands(&self) -> Vec<(&'static str, &'static str)> {
        vec![
            ("new", self.pick("Bắt đầu cuộc trò chuyện mới", "Start a new conversation")),
            ("stop", self.pick("Dừng việc đang làm, bỏ tin đang chờ", "Stop, and drop what is waiting")),
            ("last", self.pick("Gửi lại câu trả lời gần nhất", "Send the last answer again")),
            ("status", self.pick("Máy tính đang ra sao", "How the computer is doing")),
            ("help", self.pick("Bot này làm gì", "What this bot does")),
            ("unpair", self.pick("Gỡ ghép tài khoản này", "Unpair this account")),
        ]
    }

    pub fn help(&self) -> &'static str {
        self.pick(
            "Nhắn gì cũng được — Syn tự quyết: trả lời câu hỏi, hay lưu thứ bạn gửi vào QuickCap. \
             Nhờ nhắc việc cũng được: “8 giờ tối nhắc mua thuốc”.\n\n\
             /new — cuộc trò chuyện mới\n\
             /stop — dừng việc đang làm, bỏ tin đang chờ\n\
             /last — gửi lại câu trả lời gần nhất\n\
             /status — máy tính đang ra sao\n\
             /unpair — gỡ ghép tài khoản này\n\n\
             Ảnh, tệp và tin thoại gửi được, mỗi thứ tối đa 20 MB.",
            "Send anything — Syn decides: answer a question, or keep what you sent in QuickCap. \
             Ask to be reminded, too: “remind me to buy medicine at 8pm”.\n\n\
             /new — a new conversation\n\
             /stop — stop, and drop what is waiting\n\
             /last — send the last answer again\n\
             /status — how the computer is doing\n\
             /unpair — unpair this account\n\n\
             Photos, files and voice notes work too, up to 20 MB each.",
        )
    }

    pub fn paired(&self, computer: &str) -> String {
        if self.vietnamese {
            format!("Đã ghép với {computer}. Từ giờ bạn nhắn gì Syn cũng nhận.")
        } else {
            format!("Paired with {computer}. Syn will read whatever you send from now on.")
        }
    }

    pub fn unpaired(&self) -> &'static str {
        self.pick(
            "Đã gỡ ghép. Bot sẽ không trả lời tài khoản này nữa cho tới khi ghép lại từ app.",
            "Unpaired. The bot will not answer this account again until it is paired from the app.",
        )
    }

    pub fn new_conversation(&self) -> &'static str {
        self.pick("Bắt đầu cuộc trò chuyện mới.", "Started a new conversation.")
    }

    pub fn stopped(&self, dropped: usize) -> String {
        match (self.vietnamese, dropped) {
            (true, 0) => "Đã dừng.".to_string(),
            (true, n) => format!("Đã dừng, bỏ {n} tin đang chờ."),
            (false, 0) => "Stopped.".to_string(),
            (false, n) => format!("Stopped, and dropped {n} waiting message(s)."),
        }
    }

    pub fn nothing_to_stop(&self) -> &'static str {
        self.pick("Không có gì đang chạy.", "Nothing is running.")
    }

    pub fn nothing_yet(&self) -> &'static str {
        self.pick(
            "Chưa có câu trả lời nào trong cuộc trò chuyện này.",
            "There is no answer in this conversation yet.",
        )
    }

    /// What was kept, said by the app rather than by Syn.
    pub fn kept(&self, what: &[Kept]) -> String {
        let [one] = what else {
            return if self.vietnamese {
                format!("✓ Đã lưu {} mục vào QuickCap.", what.len())
            } else {
                format!("✓ Kept {} items in QuickCap.", what.len())
            };
        };
        let things = self.things(one);
        let quoted = (!one.text.is_empty()).then(|| format!("“{}”", one.text));
        match (self.vietnamese, quoted, things) {
            (true, Some(q), Some(t)) => format!("✓ Đã lưu vào QuickCap: {q}, kèm {t}"),
            (true, Some(q), None) => format!("✓ Đã lưu vào QuickCap: {q}"),
            (true, None, Some(t)) => format!("✓ Đã lưu {t} vào QuickCap"),
            (true, None, None) => "✓ Đã lưu vào QuickCap".to_string(),
            (false, Some(q), Some(t)) => format!("✓ Kept in QuickCap: {q}, with {t}"),
            (false, Some(q), None) => format!("✓ Kept in QuickCap: {q}"),
            (false, None, Some(t)) => format!("✓ Kept {t} in QuickCap"),
            (false, None, None) => "✓ Kept in QuickCap".to_string(),
        }
    }

    /// "2 ảnh, 1 tin thoại" — the files one capture kept.
    fn things(&self, kept: &Kept) -> Option<String> {
        let counted = |n: usize, vi: &str, one: &str, many: &str| match (n, self.vietnamese) {
            (0, _) => None,
            (n, true) => Some(format!("{n} {vi}")),
            (1, false) => Some(format!("1 {one}")),
            (n, false) => Some(format!("{n} {many}")),
        };
        let parts: Vec<String> = [
            counted(kept.images, "ảnh", "photo", "photos"),
            counted(kept.audio, "tin thoại", "recording", "recordings"),
            counted(kept.files, "tệp", "file", "files"),
        ]
        .into_iter()
        .flatten()
        .collect();
        (!parts.is_empty()).then(|| parts.join(", "))
    }

    pub fn not_kept(&self) -> &'static str {
        self.pick(
            "✗ Chưa lưu được vào QuickCap — thử gửi lại.",
            "✗ Nothing was kept in QuickCap — try sending it again.",
        )
    }

    /// The question on a card, when more than one note would do.
    pub fn which_one(&self, tool: &str, count: usize) -> String {
        match (self.vietnamese, tool) {
            (true, "trash_node") => format!("Có {count} ghi chú khớp. Syn nên xoá cái nào?"),
            (true, "update_node") => format!("Có {count} ghi chú khớp. Syn nên sửa cái nào?"),
            (true, _) => format!("Có {count} ghi chú khớp. Ý bạn là cái nào?"),
            (false, "trash_node") => format!("{count} notes match. Which one should Syn remove?"),
            (false, "update_node") => format!("{count} notes match. Which one should Syn change?"),
            (false, _) => format!("{count} notes match. Which one do you mean?"),
        }
    }

    pub fn none_of_these(&self) -> &'static str {
        self.pick("Không cái nào", "None of these")
    }

    /// What is sent to Syn when a note is picked on a card.
    pub fn meant(&self, title: &str, id: &str) -> String {
        if self.vietnamese {
            format!("Ý tôi là “{title}” ({id}).")
        } else {
            format!("I mean “{title}” ({id}).")
        }
    }

    /// The question on a permission card. `about` is the capability's own
    /// sentence, which is English: there is no catalogue of these outside the
    /// app, and on Telegram's tools a card like this should not come up at all.
    pub fn may_i(&self, about: &str) -> String {
        if self.vietnamese {
            format!("Syn cần quyền: {about}. Cho phép lần này?")
        } else {
            format!("Syn needs permission to {about}. Allow it this once?")
        }
    }

    pub fn allow_once(&self) -> &'static str {
        self.pick("Cho phép lần này", "Allow once")
    }

    pub fn decline(&self) -> &'static str {
        self.pick("Không", "No")
    }

    pub fn card_gone(&self) -> &'static str {
        self.pick("Câu hỏi này không còn nữa.", "This question is no longer open.")
    }

    pub fn needs_the_app(&self) -> &'static str {
        self.pick(
            "Syn dừng lại để hỏi một điều chỉ app hiện được. Mở cuộc trò chuyện trên máy tính để tiếp tục.",
            "Syn stopped to ask something only the app can show. Open the conversation on the computer to carry on.",
        )
    }

    pub fn not_ready(&self, reason: &str) -> String {
        if self.vietnamese {
            format!("Syn chưa sẵn sàng ({reason}). Tin đã được giữ lại và sẽ được xử lý khi sẵn sàng.")
        } else {
            format!("Syn is not ready ({reason}). Your message is kept and will be answered when it is.")
        }
    }

    pub fn no_vault(&self) -> &'static str {
        self.pick("chưa mở vault nào trên máy tính", "no vault is open on the computer")
    }

    pub fn syn_off(&self) -> &'static str {
        self.pick("Syn đang tắt trong cài đặt", "Syn is switched off in settings")
    }

    pub fn status(&self, computer: &str, vault_open: bool, syn_on: bool, pending: usize, background: usize) -> String {
        let mut said = if self.vietnamese {
            format!(
                "Máy: {computer}\nVault: {}\nSyn: {}\nTin đang chờ: {pending}",
                if vault_open { "đang mở" } else { "chưa mở" },
                if syn_on { "bật" } else { "tắt" },
            )
        } else {
            format!(
                "Computer: {computer}\nVault: {}\nSyn: {}\nWaiting: {pending}",
                if vault_open { "open" } else { "not open" },
                if syn_on { "on" } else { "off" },
            )
        };
        if background > 0 {
            said.push_str(&if self.vietnamese {
                format!("\nĐang làm nền: {background} việc")
            } else {
                format!("\nStill working on: {background}")
            });
        }
        said
    }

    /// Said under a question that is taking long enough to stop waiting for.
    pub fn still_working(&self) -> &'static str {
        self.pick(
            "⏳ Việc này lâu hơn chút. Syn vẫn đang làm, xong sẽ trả lời ngay dưới tin này — cứ nhắn việc khác.",
            "⏳ This one is taking a while. Syn is still on it and will reply to this message when it is done — send something else meanwhile.",
        )
    }

    /// A reminder, as the phone shows it.
    ///
    /// `late` when it is arriving well after its moment — the computer was
    /// asleep, or offline — so "starts now" is not said of something that
    /// started an hour ago.
    pub fn reminder(&self, due: &PlannedReminder, late: bool, now: NaiveDateTime) -> String {
        let vi = self.vietnamese;
        let title = &due.title;
        let when = |at: NaiveDateTime| {
            if at.date() == now.date() {
                at.format("%H:%M").to_string()
            } else {
                at.format("%H:%M %d/%m").to_string()
            }
        };
        let day = due.subject_at.format("%d/%m");
        let mut said = match (due.target_type, due.offset.as_str()) {
            ("task", _) if due.overdue => {
                if vi { format!("⏰ Quá hạn: {title}") } else { format!("⏰ Overdue: {title}") }
            }
            ("task", _) => format!("⏰ {title}"),
            ("person", "touch") => {
                if vi { format!("👋 Đến lúc hỏi thăm {title}") } else { format!("👋 Time to catch up with {title}") }
            }
            ("person", "0m") => {
                if vi { format!("🎂 Hôm nay là sinh nhật {title}") } else { format!("🎂 Today is {title}'s birthday") }
            }
            ("person", "1d") => {
                if vi { format!("🎂 Mai là sinh nhật {title}") } else { format!("🎂 Tomorrow is {title}'s birthday") }
            }
            ("person", _) => {
                if vi { format!("🎂 Sinh nhật {title} vào {day}") } else { format!("🎂 {title}'s birthday is on {day}") }
            }
            ("decision", _) => {
                if vi { format!("🪞 Nhìn lại: {title} — điều gì đã thật sự xảy ra?") } else { format!("🪞 Look back: {title} — what actually happened?") }
            }
            ("finance_debt", _) => {
                if vi { format!("💸 Đến hạn {day}: {title}") } else { format!("💸 Due {day}: {title}") }
            }
            (_, "0m") => {
                if vi { format!("📅 Bắt đầu: {title}") } else { format!("📅 Starting: {title}") }
            }
            _ => {
                let at = when(due.subject_at);
                if vi { format!("📅 {title} — lúc {at}") } else { format!("📅 {title} — at {at}") }
            }
        };
        if late {
            let at = when(due.trigger_at);
            said.push_str(&if vi {
                format!("\n(đến trễ — lẽ ra lúc {at})")
            } else {
                format!("\n(late — this was due at {at})")
            });
        }
        said
    }

    pub fn done_button(&self) -> &'static str {
        self.pick("✅ Xong", "✅ Done")
    }

    pub fn marked_done(&self) -> &'static str {
        self.pick("✅ Đã đánh dấu xong", "✅ Marked done")
    }

    pub fn task_gone(&self) -> &'static str {
        self.pick("Không còn thấy việc này trong vault.", "This task is no longer in the vault.")
    }

    pub fn guessing(&self) -> &'static str {
        self.pick("đoán", "a guess")
    }

    pub fn chart(&self) -> &'static str {
        self.pick("(biểu đồ — mở trên máy tính để xem)", "(a chart — open it on the computer)")
    }

    pub fn image(&self) -> &'static str {
        self.pick("(ảnh — mở trên máy tính để xem)", "(an image — open it on the computer)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(s: &str) -> NaiveDateTime {
        NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M").expect("a time")
    }

    fn due(target_type: &'static str, offset: &str, subject_at: &str, overdue: bool) -> PlannedReminder {
        PlannedReminder {
            target_id: "Tasks/Mua thuốc cho con.md".into(),
            target_type,
            title: "Mua thuốc cho con".into(),
            offset: offset.into(),
            occurrence_date: subject_at[..10].into(),
            trigger_at: at(subject_at),
            subject_at: at(subject_at),
            overdue,
        }
    }

    /// Says what it is and nothing it cannot stand behind — no "due at 09:00"
    /// for a task that never had a time, and "late" only when it is.
    #[test]
    fn a_reminder_reads_as_what_it_is_and_says_when_it_is_late() {
        let vi = Words { vietnamese: true };
        let now = at("2026-09-14 20:00");
        assert_eq!(vi.reminder(&due("task", "0m", "2026-09-14 20:00", false), false, now), "⏰ Mua thuốc cho con");
        assert_eq!(
            vi.reminder(&due("task", "0m", "2026-09-14 20:00", false), true, at("2026-09-14 22:15")),
            "⏰ Mua thuốc cho con\n(đến trễ — lẽ ra lúc 20:00)"
        );
        assert_eq!(
            vi.reminder(&due("task", "0m", "2026-09-13 20:00", true), false, now),
            "⏰ Quá hạn: Mua thuốc cho con"
        );

        let en = Words::english();
        let mut meeting = due("event", "15m", "2026-09-15 09:30", false);
        meeting.title = "Stand-up".into();
        assert_eq!(en.reminder(&meeting, false, now), "📅 Stand-up — at 09:30 15/09");
    }
}
