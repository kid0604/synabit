//! The one-time repairs that bring a Finance vault up to the shape the app
//! reads today.
//!
//! Three of them, all of which used to run inside `FinanceApp.vue`'s
//! `loadData()` — detected on every launch, applied to whatever was in memory,
//! and written back through the ordinary save path with a `.catch()` that only
//! reached the log. That is wrong in three separate ways, and the third is the
//! one that loses data:
//!
//! - It ran on every launch, because nothing recorded that it had finished.
//! - A failure was invisible. The screen drew from the repaired copy in memory
//!   while the disk still held the old one.
//! - It went through `write_node_file`, which stamps `updated_at` with the
//!   clock and queues a sync delta. Two devices repairing the same file
//!   therefore produced two different byte sequences for the same repair, and
//!   the CRDT was handed two independent rewrites of one document to merge.
//!
//! `commands::migration` explains that last one at length and provides the
//! write path that avoids it. Everything here is a pure function of a file's
//! bytes so that it qualifies: no clock, no fresh identifiers, no dependence
//! on which file is visited first.
//!
//! # Why the category lists are duplicated here
//!
//! They also exist in `src/mini-apps/finance/types.ts`, and they are
//! deliberately not kept in step with it. A migration describes a moment in
//! the vault's history; if its output changed when somebody added a default
//! category next year, the same file would migrate differently on two devices
//! running two app versions — which is exactly the non-determinism the whole
//! design is built to avoid. These lists are frozen.

use std::collections::HashMap;

use serde_json::{Map, Value};

/// The income categories the app shipped with when this migration was written.
const DEFAULT_INCOME: &[&str] = &[
    "Salary",
    "Bonus",
    "Allowance",
    "Savings Interest",
    "Investment Return",
    "Gift",
    "Business",
    "Freelance",
    "Borrowing",
    "Debt Collection",
    "Other Income",
];

/// The expense categories, likewise.
const DEFAULT_EXPENSE: &[&str] = &[
    "Food & Dining",
    "Transportation",
    "Bills & Utilities",
    "Housing",
    "Gifts & Donations",
    "Health & Medical",
    "Clothing",
    "Entertainment",
    "Education",
    "Family & Kids",
    "Investment",
    "Insurance",
    "Lending",
    "Debt Repayment",
    "Other Expense",
];

/// Categories the debts ledger writes into and the user may not delete.
const SYSTEM_INCOME: &[&str] = &["Borrowing", "Debt Collection"];
const SYSTEM_EXPENSE: &[&str] = &["Lending", "Debt Repayment"];

/// Amounts are minor units, and the file says so.
///
/// Kept in step with `FINANCE_SCHEMA` in `src/mini-apps/finance/schema.ts`.
pub const FINANCE_SCHEMA: u64 = 2;

const SCHEMA_KEY: &str = "financeSchema";

/// How many digits of minor unit each currency has.
///
/// Handed in by the caller rather than reimplemented here, because the
/// interface has the same table and the two multiplying by different powers of
/// ten would scale a vault wrongly with no way back. One table, one answer.
#[derive(Debug, Clone)]
pub struct Scales {
    pub table: HashMap<String, u32>,
    pub default_scale: u32,
    pub vault_currency: String,
}

impl Scales {
    fn of(&self, currency: &str) -> u32 {
        self.table
            .get(&currency.to_uppercase())
            .copied()
            .unwrap_or(self.default_scale)
    }

    fn factor(&self, currency: &str) -> f64 {
        10_f64.powi(self.of(currency) as i32)
    }

    fn vault_factor(&self) -> f64 {
        self.factor(&self.vault_currency)
    }
}

/// Whether this node's amounts are minor units already.
///
/// A file written before the marker existed does not carry one, and that
/// absence is the answer.
fn is_current_schema(meta: &Map<String, Value>) -> bool {
    meta.get(SCHEMA_KEY)
        .and_then(Value::as_u64)
        .map(|v| v >= FINANCE_SCHEMA)
        .unwrap_or(false)
}

/// Multiply one money field in place. Absent or unreadable fields are left be.
fn scale_field(holder: &mut Map<String, Value>, key: &str, factor: f64) {
    let Some(current) = holder.get(key).and_then(Value::as_f64) else {
        return;
    };
    holder.insert(
        key.to_string(),
        Value::from((current * factor).round() as i64),
    );
}

/// One account, as much of it as a migration needs.
pub struct AccountRef {
    pub id: String,
    pub name: String,
}

/// The accounts a config file lists, in the order it lists them.
///
/// The order matters: a transaction naming an account that no longer exists
/// falls back to the first one, so a different order would file it differently
/// on two devices.
pub fn accounts_in(config_contents: &str) -> Vec<AccountRef> {
    let Ok(file) = serde_json::from_str::<Value>(config_contents) else {
        return Vec::new();
    };
    let Some(accounts) = file.pointer("/metadata/accounts").and_then(Value::as_array) else {
        return Vec::new();
    };

    accounts
        .iter()
        .filter_map(|acc| {
            Some(AccountRef {
                id: acc.get("id")?.as_str()?.to_string(),
                name: acc.get("name")?.as_str()?.to_string(),
            })
        })
        .collect()
}

/// The currency a config file says the vault is kept in.
///
/// Read from the file rather than passed in, so the repair does not depend on
/// the interface having loaded Finance first — it runs before that.
pub fn currency_in(config_contents: &str) -> String {
    serde_json::from_str::<Value>(config_contents)
        .ok()
        .and_then(|file| {
            file.pointer("/metadata/currency")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .unwrap_or_else(|| "USD".to_string())
}

/// Re-serialise a node file the way the app writes one.
///
/// `serde_json` has no `preserve_order` feature here, so its maps are ordered
/// maps by key — which means parsing a file and printing it again reproduces
/// the app's own layout exactly. That is what keeps a repaired file from
/// looking changed to the next ordinary save.
fn render(file: &Value) -> Option<String> {
    serde_json::to_string_pretty(file).ok()
}

/// The `metadata` object of a node file, if it has one.
fn metadata_of(file: &mut Value) -> Option<&mut Map<String, Value>> {
    file.get_mut("metadata")?.as_object_mut()
}

/// The id of a category entry, whichever shape it is in.
fn category_id(entry: &Value) -> Option<&str> {
    match entry {
        Value::String(name) => Some(name.as_str()),
        Value::Object(map) => map.get("id").and_then(Value::as_str),
        _ => None,
    }
}

/// Give every bare string in a category list an id and a name.
///
/// Returns whether anything changed, so a list already in the new shape costs
/// a walk and no write.
fn name_categories(list: &mut Vec<Value>) -> bool {
    let mut changed = false;
    for entry in list.iter_mut() {
        let Value::String(name) = entry else { continue };
        let mut named = Map::new();
        named.insert("id".into(), Value::String(name.clone()));
        named.insert("name".into(), Value::String(name.clone()));
        *entry = Value::Object(named);
        changed = true;
    }
    changed
}

/// Append the categories in `required` that `list` does not already have.
fn ensure_present(list: &mut Vec<Value>, required: &[&str]) -> bool {
    let mut changed = false;
    for wanted in required {
        if list.iter().any(|v| category_id(v) == Some(*wanted)) {
            continue;
        }
        let mut named = Map::new();
        named.insert("id".into(), Value::String((*wanted).to_string()));
        named.insert("name".into(), Value::String((*wanted).to_string()));
        list.push(Value::Object(named));
        changed = true;
    }
    changed
}

/// Read a metadata key as a list of strings.
fn string_list(meta: &Map<String, Value>, key: &str) -> Option<Vec<String>> {
    Some(
        meta.get(key)?
            .as_array()?
            .iter()
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect(),
    )
}

fn to_values(items: Vec<String>) -> Vec<Value> {
    items.into_iter().map(Value::String).collect()
}

/// Repair `Finance/Config.json`, or report that it needs nothing.
///
/// Three transforms, in the order they have to happen:
///
/// 1. A vault old enough to have one flat `categories` list has it split into
///    an income list and an expense list. The income side becomes the defaults
///    outright, because the old list was only ever used for expenses; anything
///    in it that is not a default income category is kept on the expense side.
/// 2. The system categories — the ones the debts ledger writes into — are
///    appended if missing. The app used to add these to its own copy on every
///    launch and never write them down, so most vaults are missing them on
///    disk while appearing to have them on screen.
/// 3. Budgets saved as a flat list of allocations become one named budget
///    containing those allocations as items.
pub fn migrate_config(contents: &str, scales: &Scales) -> Option<String> {
    let mut file: Value = serde_json::from_str(contents).ok()?;
    let mut changed = false;

    {
        let meta = metadata_of(&mut file)?;

        // 1. The legacy single list.
        if let Some(legacy) = string_list(meta, "categories") {
            let income: Vec<String> = DEFAULT_INCOME.iter().map(|s| s.to_string()).collect();

            let mut expense: Vec<String> = DEFAULT_EXPENSE.iter().map(|s| s.to_string()).collect();
            for cat in legacy {
                if !DEFAULT_INCOME.contains(&cat.as_str()) && !expense.contains(&cat) {
                    expense.push(cat);
                }
            }

            meta.remove("categories");
            meta.insert("incomeCategories".into(), Value::Array(to_values(income)));
            meta.insert("expenseCategories".into(), Value::Array(to_values(expense)));
            changed = true;
        }

        // 2. A category becomes something that can be renamed.
        //
        // The id of every category that already exists is its old name. That is
        // not laziness: a transaction already holds that string, so choosing
        // anything else would mean rewriting every transaction in the vault to
        // point at a new id. Renaming from here on changes `name` and leaves
        // `id` alone, and the history follows.
        for key in ["incomeCategories", "expenseCategories"] {
            if let Some(Value::Array(list)) = meta.get_mut(key) {
                if name_categories(list) {
                    changed = true;
                }
            }
        }

        // 3. The system categories, on whichever lists exist.
        for (key, required) in [
            ("incomeCategories", SYSTEM_INCOME),
            ("expenseCategories", SYSTEM_EXPENSE),
        ] {
            if let Some(Value::Array(list)) = meta.get_mut(key) {
                if ensure_present(list, required) {
                    changed = true;
                }
            }
        }

        // 3. Budgets that are allocations rather than budgets.
        if let Some(Value::Array(budgets)) = meta.get_mut("budgets") {
            if is_flat_budget_list(budgets) {
                let items = budgets
                    .iter()
                    .enumerate()
                    .map(|(index, item)| legacy_budget_item(index, item))
                    .collect();

                let mut container = Map::new();
                container.insert("id".into(), Value::String("budget-default-monthly".into()));
                container.insert("name".into(), Value::String("Monthly Budget".into()));
                container.insert("type".into(), Value::String("monthly".into()));
                container.insert("items".into(), Value::Array(items));

                *budgets = vec![Value::Object(container)];
                changed = true;
            }
        }
    }

    // 4. Whole units to minor units. Last, so that the budget items created
    //    above are scaled along with everything else.
    {
        let meta = metadata_of(&mut file)?;
        if !is_current_schema(meta) {
            let factor = scales.vault_factor();

            if let Some(Value::Array(accounts)) = meta.get_mut("accounts") {
                for account in accounts.iter_mut() {
                    if let Some(account) = account.as_object_mut() {
                        scale_field(account, "initialBalance", factor);
                    }
                }
            }

            if let Some(Value::Array(budgets)) = meta.get_mut("budgets") {
                for budget in budgets.iter_mut() {
                    let Some(items) = budget.get_mut("items").and_then(Value::as_array_mut) else {
                        continue;
                    };
                    for item in items.iter_mut() {
                        let Some(item) = item.as_object_mut() else { continue };
                        scale_field(item, "amount", factor);
                        if let Some(Value::Object(overrides)) = item.get_mut("monthlyOverrides") {
                            let months: Vec<String> = overrides.keys().cloned().collect();
                            for month in months {
                                scale_field(overrides, &month, factor);
                            }
                        }
                    }
                }
            }

            meta.insert(SCHEMA_KEY.to_string(), Value::from(FINANCE_SCHEMA));
            changed = true;
        }
    }

    if !changed {
        return None;
    }
    render(&file)
}

/// Whether a `budgets` array holds allocations rather than named budgets.
///
/// An allocation has no `items`; a budget does. Only the first entry is
/// consulted because the two shapes never coexisted — a vault is entirely one
/// or entirely the other.
fn is_flat_budget_list(budgets: &[Value]) -> bool {
    budgets
        .first()
        .map(|b| b.get("items").is_none())
        .unwrap_or(false)
}

/// One legacy allocation, as a budget item.
///
/// The identifier is derived from the position rather than minted, which is
/// the whole reason this can run on two devices at once. The original code
/// used `Date.now()` and `Math.random()`, so the same budget would have come
/// out with a different identifier on every device that opened the vault.
fn legacy_budget_item(index: usize, item: &Value) -> Value {
    let mut out = Map::new();

    let id = item
        .get("id")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| format!("bi-legacy-{index}"));
    out.insert("id".into(), Value::String(id));

    let name = item
        .get("name")
        .and_then(Value::as_str)
        .or_else(|| item.get("categoryId").and_then(Value::as_str))
        .unwrap_or("")
        .to_string();
    out.insert("name".into(), Value::String(name));

    let categories = match item.get("categories").and_then(Value::as_array) {
        Some(list) => list.clone(),
        None => match item.get("categoryId") {
            Some(Value::String(single)) => vec![Value::String(single.clone())],
            _ => Vec::new(),
        },
    };
    out.insert("categories".into(), Value::Array(categories));

    let amount = item.get("amount").cloned().unwrap_or(Value::from(0));
    out.insert("amount".into(), amount);

    if let Some(overrides) = item.get("monthlyOverrides") {
        out.insert("monthlyOverrides".into(), overrides.clone());
    }

    Value::Object(out)
}

/// Repair one `Finance/YYYY-MM.json`, or report that it needs nothing.
///
/// The only transform is the oldest one in the app: transactions used to name
/// their account by its *name*, which broke the moment somebody renamed an
/// account. They are matched back to an identifier here.
///
/// A transaction naming an account that no longer exists falls back to the
/// first account, matching what the app did — except that with no accounts at
/// all it is left exactly as it was rather than having its only record of
/// where the money went deleted.
pub fn migrate_month(contents: &str, accounts: &[AccountRef], scales: &Scales) -> Option<String> {
    let mut file: Value = serde_json::from_str(contents).ok()?;
    let mut changed = false;

    let to_minor_units = !is_current_schema(metadata_of(&mut file)?);

    // 1. An account named by its name becomes one named by its identifier.
    if let Some(Value::Array(transactions)) = metadata_of(&mut file)?.get_mut("transactions") {
        for tx in transactions.iter_mut() {
            let Some(tx) = tx.as_object_mut() else { continue };

            if tx.contains_key("accountId") {
                // Already migrated. A stray `account` alongside it is the
                // remains of a half-finished pass; drop it.
                if tx.remove("account").is_some() {
                    changed = true;
                }
                continue;
            }

            let Some(name) = tx.get("account").and_then(Value::as_str) else {
                continue;
            };

            let matched = accounts
                .iter()
                .find(|acc| acc.name == name)
                .or_else(|| accounts.first());

            let Some(account) = matched else { continue };

            let id = account.id.clone();
            tx.remove("account");
            tx.insert("accountId".into(), Value::String(id));
            changed = true;
        }
    }

    // 2. Whole units become minor units.
    if to_minor_units {
        let vault_factor = scales.vault_factor();

        if let Some(Value::Array(transactions)) = metadata_of(&mut file)?.get_mut("transactions") {
            for tx in transactions.iter_mut() {
                let Some(tx) = tx.as_object_mut() else { continue };
                scale_field(tx, "amount", vault_factor);

                // What the user originally typed, in the currency they typed
                // it in — which may carry a different number of digits than
                // the vault's own.
                if tx.contains_key("originalAmount") {
                    let original = tx
                        .get("originalCurrency")
                        .and_then(Value::as_str)
                        .unwrap_or(&scales.vault_currency)
                        .to_string();
                    scale_field(tx, "originalAmount", scales.factor(&original));
                }
                // `exchangeRate` is a ratio between whole units, not money.
            }
        }

        // Stamped even on a month with no transactions in it. Without the
        // marker the reader would scale its amounts again on every load, and
        // the first transaction added to it would be a hundred times too big.
        metadata_of(&mut file)?.insert(SCHEMA_KEY.to_string(), Value::from(FINANCE_SCHEMA));
        changed = true;
    }

    if !changed {
        return None;
    }
    render(&file)
}

/// Repair `Finance/Debts.json`: the amounts owed and repaid.
///
/// The debts ledger has no shape migration of its own — it has only ever held
/// a list of debts — so this is purely the move to minor units.
pub fn migrate_debts(contents: &str, scales: &Scales) -> Option<String> {
    let mut file: Value = serde_json::from_str(contents).ok()?;

    {
        let meta = metadata_of(&mut file)?;
        if is_current_schema(meta) {
            return None;
        }

        let factor = scales.vault_factor();
        if let Some(Value::Array(debts)) = meta.get_mut("debts") {
            for debt in debts.iter_mut() {
                let Some(debt) = debt.as_object_mut() else { continue };
                scale_field(debt, "totalAmount", factor);
                scale_field(debt, "paidAmount", factor);
            }
        }

        meta.insert(SCHEMA_KEY.to_string(), Value::from(FINANCE_SCHEMA));
    }

    render(&file)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A config file as it sits on disk.
    fn config(metadata: Value) -> String {
        serde_json::to_string_pretty(&serde_json::json!({
            "title": "Finance Config",
            "type": "finance_config",
            "metadata": metadata,
            "content": ""
        }))
        .unwrap()
    }

    fn month(transactions: Value) -> String {
        serde_json::to_string_pretty(&serde_json::json!({
            "title": "Month 08/2026",
            "type": "finance_month",
            "metadata": { "transactions": transactions },
            "content": ""
        }))
        .unwrap()
    }

    fn stamped_month(transactions: Value) -> String {
        serde_json::to_string_pretty(&serde_json::json!({
            "title": "Month 08/2026",
            "type": "finance_month",
            "metadata": { "transactions": transactions, "financeSchema": FINANCE_SCHEMA },
            "content": ""
        }))
        .unwrap()
    }

    fn meta_of(rendered: &str) -> Value {
        serde_json::from_str::<Value>(rendered).unwrap()["metadata"].clone()
    }

    /// The names of a category list, whichever shape it is in.
    fn strings(v: &Value) -> Vec<String> {
        v.as_array()
            .unwrap()
            .iter()
            .map(|entry| match entry {
                Value::String(name) => name.clone(),
                other => other["name"].as_str().unwrap().to_string(),
            })
            .collect()
    }

    /// A category list in the shape the app writes today.
    fn named(entries: &[&str]) -> Value {
        Value::Array(
            entries
                .iter()
                .map(|name| serde_json::json!({ "id": name, "name": name }))
                .collect(),
        )
    }

    fn two_accounts() -> Vec<AccountRef> {
        vec![
            AccountRef { id: "acc-1".into(), name: "Cash".into() },
            AccountRef { id: "acc-2".into(), name: "Bank Account".into() },
        ]
    }

    /// The table the interface hands in, for a vault kept in the named
    /// currency. Only the currencies that are not two-digit appear in it.
    fn scales(vault: &str) -> Scales {
        Scales {
            table: HashMap::from([
                ("VND".to_string(), 0),
                ("JPY".to_string(), 0),
                ("KWD".to_string(), 3),
            ]),
            default_scale: 2,
            vault_currency: vault.to_string(),
        }
    }

    /// A vault kept in dollars: a hundred minor units to the dollar.
    fn usd() -> Scales {
        scales("USD")
    }

    /// A file that already carries the marker, so the amount pass leaves it be.
    fn stamped(mut metadata: Value) -> Value {
        metadata
            .as_object_mut()
            .unwrap()
            .insert("financeSchema".into(), Value::from(FINANCE_SCHEMA));
        metadata
    }

    // ---- the property the whole design rests on -------------------------

    /// Running the repair on its own output must change nothing, or a device
    /// that migrates twice writes a different file from one that migrated once.
    #[test]
    fn a_second_pass_over_a_repaired_config_changes_nothing() {
        let legacy = config(serde_json::json!({
            "categories": ["Food & Dining", "Coffee"],
            "accounts": [{ "id": "acc-1", "name": "Cash", "initialBalance": 0 }],
            "budgets": [{ "categoryId": "Coffee", "amount": 500 }],
        }));

        let once = migrate_config(&legacy, &usd()).expect("first pass repairs");
        assert_eq!(migrate_config(&once, &usd()), None, "second pass wanted to change it");
    }

    #[test]
    fn a_second_pass_over_a_repaired_month_changes_nothing() {
        let legacy = month(serde_json::json!([
            { "id": "tx-1", "type": "expense", "amount": 50, "account": "Cash" }
        ]));

        let once = migrate_month(&legacy, &two_accounts(), &usd()).expect("first pass repairs");
        assert_eq!(migrate_month(&once, &two_accounts(), &usd()), None);
    }

    /// Two devices, same file, same repair. If these ever differ the CRDT is
    /// handed two rewrites of one document.
    #[test]
    fn two_devices_produce_identical_bytes() {
        let legacy = config(serde_json::json!({
            "budgets": [
                { "name": "Eating out", "categories": ["Food & Dining"], "amount": 3000 },
                { "name": "Petrol", "categories": ["Transportation"], "amount": 1000 },
            ],
        }));

        assert_eq!(migrate_config(&legacy, &usd()), migrate_config(&legacy, &usd()));
    }

    /// The identifier a legacy budget item gets must come from the file, not
    /// from a clock or a random number.
    #[test]
    fn a_legacy_budget_item_is_identified_by_its_position() {
        let legacy = config(serde_json::json!({
            "budgets": [
                { "name": "Eating out", "categories": ["Food & Dining"], "amount": 3000 },
                { "name": "Petrol", "categories": ["Transportation"], "amount": 1000 },
            ],
        }));

        let meta = meta_of(&migrate_config(&legacy, &usd()).unwrap());
        let items = meta["budgets"][0]["items"].as_array().unwrap();

        assert_eq!(items[0]["id"], "bi-legacy-0");
        assert_eq!(items[1]["id"], "bi-legacy-1");
    }

    // ---- config: the legacy category list --------------------------------

    #[test]
    fn one_category_list_becomes_two() {
        let legacy = config(serde_json::json!({ "categories": ["Food & Dining", "Coffee"] }));
        let meta = meta_of(&migrate_config(&legacy, &usd()).unwrap());

        assert!(meta.get("categories").is_none(), "old key survived: {meta}");
        assert_eq!(strings(&meta["incomeCategories"]), DEFAULT_INCOME.to_vec());
    }

    /// The user's own categories are the only reason this migration is not a
    /// straight overwrite. Losing them would lose the history filed under them.
    #[test]
    fn a_category_the_user_invented_is_kept() {
        let legacy = config(serde_json::json!({ "categories": ["Cà phê", "Food & Dining"] }));
        let meta = meta_of(&migrate_config(&legacy, &usd()).unwrap());

        assert!(strings(&meta["expenseCategories"]).contains(&"Cà phê".to_string()));
    }

    #[test]
    fn a_default_category_is_not_listed_twice() {
        let legacy = config(serde_json::json!({ "categories": ["Food & Dining"] }));
        let meta = meta_of(&migrate_config(&legacy, &usd()).unwrap());

        let expense = strings(&meta["expenseCategories"]);
        let food = expense.iter().filter(|c| *c == "Food & Dining").count();
        assert_eq!(food, 1, "in {expense:?}");
    }

    /// An old list that happens to contain an income category does not drag it
    /// onto the expense side, where it would offer "Salary" as a way to spend.
    #[test]
    fn an_income_category_does_not_cross_over() {
        let legacy = config(serde_json::json!({ "categories": ["Salary", "Coffee"] }));
        let meta = meta_of(&migrate_config(&legacy, &usd()).unwrap());

        assert!(!strings(&meta["expenseCategories"]).contains(&"Salary".to_string()));
    }

    // ---- config: the system categories ------------------------------------

    #[test]
    fn the_debts_ledger_categories_are_added_if_missing() {
        let stripped = config(serde_json::json!({
            "incomeCategories": ["Salary"],
            "expenseCategories": ["Food & Dining"],
        }));
        let meta = meta_of(&migrate_config(&stripped, &usd()).unwrap());

        assert_eq!(strings(&meta["incomeCategories"]), vec!["Salary", "Borrowing", "Debt Collection"]);
        assert_eq!(
            strings(&meta["expenseCategories"]),
            vec!["Food & Dining", "Lending", "Debt Repayment"]
        );
    }

    #[test]
    fn a_config_that_needs_nothing_is_left_alone() {
        let current = config(stamped(serde_json::json!({
            "incomeCategories": named(&["Salary", "Borrowing", "Debt Collection"]),
            "expenseCategories": named(&["Food & Dining", "Lending", "Debt Repayment"]),
            "accounts": [{ "id": "acc-1", "name": "Cash", "initialBalance": 0 }],
        })));

        assert_eq!(migrate_config(&current, &usd()), None);
    }

    // ---- config: categories become renameable --------------------------------

    /// The id has to be the old name, because that is the string every
    /// transaction in the vault already holds. Choosing anything else would
    /// mean rewriting all of them.
    #[test]
    fn a_category_keeps_its_old_name_as_its_id() {
        let legacy = config(serde_json::json!({
            "expenseCategories": ["Food & Dining"],
        }));
        let meta = meta_of(&migrate_config(&legacy, &usd()).unwrap());

        assert_eq!(meta["expenseCategories"][0]["id"], "Food & Dining");
        assert_eq!(meta["expenseCategories"][0]["name"], "Food & Dining");
    }

    /// A rename the user has already made must survive the repair untouched.
    #[test]
    fn a_category_already_renamed_is_left_alone() {
        let current = config(stamped(serde_json::json!({
            "incomeCategories": [
                { "id": "Salary", "name": "Lương" },
                { "id": "Borrowing", "name": "Borrowing" },
                { "id": "Debt Collection", "name": "Debt Collection" },
            ],
            "expenseCategories": named(&["Lending", "Debt Repayment"]),
        })));

        assert_eq!(migrate_config(&current, &usd()), None);
    }

    /// What a repair interrupted halfway leaves behind.
    #[test]
    fn a_half_named_list_is_finished() {
        let half = config(serde_json::json!({
            "expenseCategories": [
                { "id": "Food & Dining", "name": "Ăn uống" },
                "Transportation",
            ],
        }));
        let meta = meta_of(&migrate_config(&half, &usd()).unwrap());

        assert_eq!(meta["expenseCategories"][0]["name"], "Ăn uống", "the rename was undone");
        assert_eq!(meta["expenseCategories"][1]["id"], "Transportation");
    }

    /// The debts ledger's categories are matched by id, so adding them to a
    /// list that already has them under a translated name does not duplicate.
    #[test]
    fn a_renamed_system_category_is_not_added_again() {
        let current = config(serde_json::json!({
            "expenseCategories": [
                { "id": "Lending", "name": "Cho vay" },
                { "id": "Debt Repayment", "name": "Trả nợ" },
            ],
        }));
        let meta = meta_of(&migrate_config(&current, &usd()).unwrap());

        assert_eq!(meta["expenseCategories"].as_array().unwrap().len(), 2);
        assert_eq!(meta["expenseCategories"][0]["name"], "Cho vay");
    }

    // ---- config: budgets ---------------------------------------------------

    #[test]
    fn flat_allocations_become_one_named_budget() {
        let legacy = config(serde_json::json!({
            "budgets": [{ "name": "Eating out", "categories": ["Food & Dining"], "amount": 3000 }],
        }));
        let meta = meta_of(&migrate_config(&legacy, &usd()).unwrap());

        let budgets = meta["budgets"].as_array().unwrap();
        assert_eq!(budgets.len(), 1);
        assert_eq!(budgets[0]["id"], "budget-default-monthly");
        assert_eq!(budgets[0]["type"], "monthly");
        // 3,000 dollars, in cents.
        assert_eq!(budgets[0]["items"][0]["amount"], 300_000);
    }

    /// The oldest shape of all named a single category rather than a list.
    #[test]
    fn a_single_category_allocation_becomes_a_list_of_one() {
        let legacy = config(serde_json::json!({
            "budgets": [{ "categoryId": "Coffee", "amount": 500 }],
        }));
        let meta = meta_of(&migrate_config(&legacy, &usd()).unwrap());
        let item = &meta["budgets"][0]["items"][0];

        assert_eq!(strings(&item["categories"]), vec!["Coffee"]);
        assert_eq!(item["name"], "Coffee");
    }

    #[test]
    fn a_per_month_limit_survives_the_move() {
        let legacy = config(serde_json::json!({
            "budgets": [{
                "name": "Eating out",
                "categories": ["Food & Dining"],
                "amount": 3000,
                "monthlyOverrides": { "2026-08": 5000 },
            }],
        }));
        let meta = meta_of(&migrate_config(&legacy, &usd()).unwrap());

        assert_eq!(meta["budgets"][0]["items"][0]["monthlyOverrides"]["2026-08"], 500_000);
    }

    #[test]
    fn budgets_already_in_the_new_shape_are_left_alone() {
        let current = config(stamped(serde_json::json!({
            "incomeCategories": named(&["Borrowing", "Debt Collection"]),
            "expenseCategories": named(&["Lending", "Debt Repayment"]),
            "budgets": [{ "id": "b1", "name": "Monthly Budget", "type": "monthly", "items": [] }],
        })));

        assert_eq!(migrate_config(&current, &usd()), None);
    }

    // ---- months -------------------------------------------------------------

    #[test]
    fn an_account_name_becomes_an_account_id() {
        let legacy = month(serde_json::json!([
            { "id": "tx-1", "type": "expense", "amount": 50, "account": "Bank Account" }
        ]));
        let meta = meta_of(&migrate_month(&legacy, &two_accounts(), &usd()).unwrap());

        assert_eq!(meta["transactions"][0]["accountId"], "acc-2");
        assert!(meta["transactions"][0].get("account").is_none());
    }

    #[test]
    fn a_name_no_account_answers_to_falls_back_to_the_first() {
        let legacy = month(serde_json::json!([
            { "id": "tx-1", "type": "expense", "amount": 50, "account": "Ví cũ" }
        ]));
        let meta = meta_of(&migrate_month(&legacy, &two_accounts(), &usd()).unwrap());

        assert_eq!(meta["transactions"][0]["accountId"], "acc-1");
    }

    /// Better a transaction that still says "Cash" than one that says nothing.
    /// The old code deleted the name and wrote `undefined`, which JSON drops.
    #[test]
    fn a_vault_with_no_accounts_keeps_the_name_it_has() {
        let legacy = month(serde_json::json!([
            { "id": "tx-1", "type": "expense", "amount": 50, "account": "Cash" }
        ]));

        // The amount pass still runs, so the file is rewritten — but the only
        // thing it knows about where the money went stays where it is.
        let meta = meta_of(&migrate_month(&legacy, &[], &usd()).unwrap());
        assert_eq!(meta["transactions"][0]["account"], "Cash");
        assert!(meta["transactions"][0].get("accountId").is_none());
    }

    #[test]
    fn a_month_already_migrated_is_left_alone() {
        let current = stamped_month(serde_json::json!([
            { "id": "tx-1", "type": "expense", "amount": 5000, "accountId": "acc-1" }
        ]));

        assert_eq!(migrate_month(&current, &two_accounts(), &usd()), None);
    }

    /// A pass interrupted between writing `accountId` and dropping `account`.
    #[test]
    fn a_half_migrated_transaction_is_finished() {
        let half = month(serde_json::json!([
            { "id": "tx-1", "type": "expense", "amount": 50, "account": "Cash", "accountId": "acc-1" }
        ]));
        let meta = meta_of(&migrate_month(&half, &two_accounts(), &usd()).unwrap());

        assert_eq!(meta["transactions"][0]["accountId"], "acc-1");
        assert!(meta["transactions"][0].get("account").is_none());
    }

    /// An empty month still needs the marker: without it the reader would
    /// treat the first transaction added to it as whole units.
    #[test]
    fn an_empty_month_is_marked_even_though_it_holds_nothing() {
        let repaired = migrate_month(&month(serde_json::json!([])), &two_accounts(), &usd()).unwrap();
        assert_eq!(meta_of(&repaired)["financeSchema"], FINANCE_SCHEMA);
        assert_eq!(migrate_month(&repaired, &two_accounts(), &usd()), None);
    }

    #[test]
    fn everything_else_about_a_transaction_is_carried_across() {
        let legacy = month(serde_json::json!([{
            "id": "tx-1",
            "type": "expense",
            "amount": 50,
            "account": "Cash",
            "category": "Food & Dining",
            "note": "lunch",
            "personId": "People/mai.md",
            "date": "2026-08-15T10:00:00.000Z",
        }]));
        let meta = meta_of(&migrate_month(&legacy, &two_accounts(), &usd()).unwrap());
        let tx = &meta["transactions"][0];

        assert_eq!(tx["note"], "lunch");
        assert_eq!(tx["personId"], "People/mai.md");
        assert_eq!(tx["date"], "2026-08-15T10:00:00.000Z");
    }

    // ---- whole units to minor units ------------------------------------------

    #[test]
    fn a_dollar_amount_becomes_cents() {
        let legacy = month(serde_json::json!([
            { "id": "tx-1", "type": "expense", "amount": 50, "accountId": "acc-1" }
        ]));
        let meta = meta_of(&migrate_month(&legacy, &two_accounts(), &usd()).unwrap());

        assert_eq!(meta["transactions"][0]["amount"], 5000);
        assert_eq!(meta["financeSchema"], FINANCE_SCHEMA);
    }

    /// Đồng has no subunit, so the numbers do not move — but the marker does,
    /// or every later load would scale them again.
    #[test]
    fn a_dong_amount_is_unchanged_but_still_marked() {
        let legacy = month(serde_json::json!([
            { "id": "tx-1", "type": "expense", "amount": 150_000, "accountId": "acc-1" }
        ]));
        let meta = meta_of(&migrate_month(&legacy, &two_accounts(), &scales("VND")).unwrap());

        assert_eq!(meta["transactions"][0]["amount"], 150_000);
        assert_eq!(meta["financeSchema"], FINANCE_SCHEMA);
    }

    #[test]
    fn a_three_digit_currency_gains_three_digits() {
        let legacy = month(serde_json::json!([
            { "id": "tx-1", "type": "expense", "amount": 7, "accountId": "acc-1" }
        ]));
        let meta = meta_of(&migrate_month(&legacy, &two_accounts(), &scales("KWD")).unwrap());

        assert_eq!(meta["transactions"][0]["amount"], 7000);
    }

    /// What the user originally typed was in *their* currency, which may round
    /// to a different number of digits than the vault's.
    #[test]
    fn a_foreign_amount_is_scaled_by_its_own_currency() {
        let legacy = month(serde_json::json!([{
            "id": "tx-1",
            "type": "expense",
            "amount": 4,
            "accountId": "acc-1",
            "originalCurrency": "VND",
            "originalAmount": 100_000,
            "exchangeRate": 0.00004,
        }]));
        let meta = meta_of(&migrate_month(&legacy, &two_accounts(), &usd()).unwrap());
        let tx = &meta["transactions"][0];

        assert_eq!(tx["amount"], 400, "the vault side is dollars, so cents");
        assert_eq!(tx["originalAmount"], 100_000, "đồng has no subunit");
        assert_eq!(tx["exchangeRate"], 0.00004, "a rate is not money");
    }

    #[test]
    fn an_opening_balance_and_a_budget_are_scaled_together() {
        let legacy = config(serde_json::json!({
            "accounts": [{ "id": "acc-1", "name": "Cash", "initialBalance": 500 }],
            "budgets": [{
                "id": "b1",
                "name": "Monthly Budget",
                "type": "monthly",
                "items": [{
                    "id": "bi-1",
                    "name": "Eating out",
                    "categories": ["Food & Dining"],
                    "amount": 300,
                    "monthlyOverrides": { "2026-08": 450 },
                }],
            }],
        }));
        let meta = meta_of(&migrate_config(&legacy, &usd()).unwrap());

        assert_eq!(meta["accounts"][0]["initialBalance"], 50_000);
        assert_eq!(meta["budgets"][0]["items"][0]["amount"], 30_000);
        assert_eq!(meta["budgets"][0]["items"][0]["monthlyOverrides"]["2026-08"], 45_000);
    }

    /// A credit card opens below zero, and the sign has to survive.
    #[test]
    fn a_negative_opening_balance_stays_negative() {
        let legacy = config(serde_json::json!({
            "accounts": [{ "id": "acc-1", "name": "Credit Card", "initialBalance": -1_200 }],
        }));
        let meta = meta_of(&migrate_config(&legacy, &usd()).unwrap());

        assert_eq!(meta["accounts"][0]["initialBalance"], -120_000);
    }

    #[test]
    fn debts_are_scaled_and_marked() {
        let legacy = serde_json::to_string_pretty(&serde_json::json!({
            "title": "Debts Ledger",
            "type": "finance_debts",
            "metadata": { "debts": [
                { "id": "d1", "totalAmount": 500, "paidAmount": 125, "status": "active" }
            ]},
            "content": ""
        }))
        .unwrap();

        let meta = meta_of(&migrate_debts(&legacy, &usd()).unwrap());
        assert_eq!(meta["debts"][0]["totalAmount"], 50_000);
        assert_eq!(meta["debts"][0]["paidAmount"], 12_500);
        assert_eq!(meta["financeSchema"], FINANCE_SCHEMA);
    }

    #[test]
    fn a_debts_ledger_already_marked_is_left_alone() {
        let current = serde_json::to_string_pretty(&serde_json::json!({
            "title": "Debts Ledger",
            "type": "finance_debts",
            "metadata": { "debts": [], "financeSchema": FINANCE_SCHEMA },
            "content": ""
        }))
        .unwrap();

        assert_eq!(migrate_debts(&current, &usd()), None);
    }

    /// The property that makes the whole thing safe to run more than once.
    /// A second multiplication would put every amount up by a hundred.
    #[test]
    fn scaling_never_happens_twice() {
        let legacy = config(serde_json::json!({
            "accounts": [{ "id": "acc-1", "name": "Cash", "initialBalance": 500 }],
        }));
        let once = migrate_config(&legacy, &usd()).unwrap();
        assert_eq!(migrate_config(&once, &usd()), None);

        let legacy_month = month(serde_json::json!([
            { "id": "tx-1", "type": "expense", "amount": 50, "accountId": "acc-1" }
        ]));
        let once_month = migrate_month(&legacy_month, &two_accounts(), &usd()).unwrap();
        assert_eq!(migrate_month(&once_month, &two_accounts(), &usd()), None);
    }

    /// Both halves of the config migration on one file: the shape changes and
    /// the amounts move, and the item that only exists because of the first
    /// still gets scaled by the second.
    #[test]
    fn a_flattened_budget_item_is_scaled_as_well_as_reshaped() {
        let legacy = config(serde_json::json!({
            "budgets": [{ "name": "Eating out", "categories": ["Food & Dining"], "amount": 300 }],
        }));
        let meta = meta_of(&migrate_config(&legacy, &usd()).unwrap());

        assert_eq!(meta["budgets"][0]["items"][0]["id"], "bi-legacy-0");
        assert_eq!(meta["budgets"][0]["items"][0]["amount"], 30_000);
    }

    // ---- accounts_in ---------------------------------------------------------

    #[test]
    fn the_accounts_are_read_in_the_order_the_file_lists_them() {
        let file = config(serde_json::json!({
            "accounts": [
                { "id": "acc-2", "name": "Bank Account", "initialBalance": 0 },
                { "id": "acc-1", "name": "Cash", "initialBalance": 0 },
            ],
        }));

        let accounts = accounts_in(&file);
        assert_eq!(accounts[0].id, "acc-2");
        assert_eq!(accounts[1].id, "acc-1");
    }

    #[test]
    fn the_currency_is_read_from_the_config() {
        let file = config(serde_json::json!({ "currency": "VND" }));
        assert_eq!(currency_in(&file), "VND");
    }

    /// A vault written before the currency was recorded. Dollars is the
    /// interface's own default, so the two agree about what it meant.
    #[test]
    fn a_config_with_no_currency_is_assumed_to_be_dollars() {
        assert_eq!(currency_in(&config(serde_json::json!({}))), "USD");
        assert_eq!(currency_in("not json"), "USD");
    }

    #[test]
    fn a_config_with_no_accounts_yields_none() {
        assert!(accounts_in(&config(serde_json::json!({}))).is_empty());
        assert!(accounts_in("not json at all").is_empty());
    }

    // ---- files this has no business touching ----------------------------------

    #[test]
    fn a_file_that_is_not_json_is_refused() {
        assert_eq!(migrate_config("---\ntitle: a note\n---\nbody", &usd()), None);
        assert_eq!(migrate_month("{ broken", &two_accounts(), &usd()), None);
    }
}
