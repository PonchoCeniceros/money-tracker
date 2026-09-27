//! The hosted Supabase backend, talking PostgREST over the publishable key +
//! access-token headers.
//!
//! Read primitives map to the `account_balances` / `entries_view` views
//! (derived balances); every write goes through the
//! schema's `apply_entries` RPC (atomic, server-side overdraft checks) or a
//! plain insert/update/delete for the auxiliary tables. All rows are scoped
//! to the authenticated user by RLS, so nothing here ever sends a user_id.
//!
//! The session is `AuthSession` held in memory; a 401 triggers one refresh
//! from the persisted refresh token, and only fails through as
//! [`AppError::InvalidGrant`] if the token itself is dead.

use std::sync::Mutex;

use serde_json::Value;

use crate::auth::{AuthSession, SupabaseAuth};
use crate::error::{AppError, Result};
use crate::models::{
    Account, AccountKind, AccountBalance, Budget, Concept, Config, Entry, EntryFilter, EntryKind,
    NewAccount, NewEntry, EntryUpdate,
};
use crate::storage::LedgerBackend;

pub struct SupabaseBackend {
    client: reqwest::blocking::Client,
    base_url: String,
    publishable_key: String,
    auth: SupabaseAuth,
    session: Mutex<Option<AuthSession>>,
}

impl SupabaseBackend {
    /// `url` and `key` must be the project URL and its `sb_publishable_*`
    /// key (the publishable anon/publishable key, which sends the auth
    /// header via Bearer; never the service-role secret).
    pub fn new(url: &str, publishable_key: &str) -> Self {
        SupabaseBackend {
            client: reqwest::blocking::Client::new(),
            base_url: url.trim_end_matches('/').to_string(),
            publishable_key: publishable_key.to_string(),
            auth: SupabaseAuth::new(url, publishable_key),
            session: Mutex::new(None),
        }
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// `sync_state.revision` read directly; only used as the pre-0002 fallback
    /// of `status()`, when the `ledger_status()` RPC doesn't exist yet.
    fn remote_revision(&self) -> Result<i64> {
        let q = vec![
            ("select".to_string(), "revision".to_string()),
            ("id".to_string(), "eq.1".to_string()),
        ];
        let v = self.call(reqwest::Method::GET, "/rest/v1/sync_state", &q, None, None)?;
        Ok(v.as_array()
            .and_then(|a| a.first())
            .and_then(|r| r["revision"].as_i64())
            .unwrap_or(0))
    }

    fn access_token(&self) -> Result<String> {
        if let Some(s) = self.session.lock().unwrap().as_ref() {
            return Ok(s.access_token.clone());
        }
        // No in-memory session: try to resume from the persisted refresh
        // token without making the caller prompt again.
        let refresh = self.auth.load_refresh_token()?.ok_or_else(|| {
            AppError::Auth(
                "No hay sesión activa. Corre: money-tracker db remote login".into(),
            )
        })?;
        self.refresh_session(&refresh)
    }

    fn refresh_session(&self, refresh_token: &str) -> Result<String> {
        let session = self.auth.refresh(refresh_token)?;
        let token = session.access_token.clone();
        *self.session.lock().unwrap() = Some(session);
        Ok(token)
    }

    /// Sends the request with the given bearer token. On 401 and a surviving
    /// refresh token, refreshes once and retries; a second 401 falls through
    /// to the caller (which maps it, usually InvalidGrant).
    fn response(
        &self,
        method: reqwest::Method,
        path: &str,
        query: &[(String, String)],
        body: Option<Value>,
        prefer: Option<&str>,
    ) -> Result<reqwest::blocking::Response> {
        let token = self.access_token()?;
        self.raw_response(method, path, query, body, prefer, &token, false)
    }

    #[allow(clippy::too_many_arguments)]
    fn raw_response(
        &self,
        method: reqwest::Method,
        path: &str,
        query: &[(String, String)],
        body: Option<Value>,
        prefer: Option<&str>,
        token: &str,
        retried: bool,
    ) -> Result<reqwest::blocking::Response> {
        let url = format!("{}{}", self.base_url, path);
        let mut req = self
            .client
            .request(method.clone(), &url)
            .header("apikey", &self.publishable_key)
            .header("Authorization", format!("Bearer {token}"));
        if !query.is_empty() {
            let owned: Vec<(&str, &str)> = query.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
            req = req.query(&owned);
        }
        if let Some(b) = &body {
            req = req.json(b);
        }
        if let Some(pref) = prefer {
            req = req.header("Prefer", pref);
        }
        let resp = req.send()?;

        if resp.status() == reqwest::StatusCode::UNAUTHORIZED && !retried {
            let refresh = self.auth.load_refresh_token().ok().flatten();
            if let Some(refresh) = refresh {
                match self.refresh_session(&refresh) {
                    Ok(new_token) => {
                        return self.raw_response(
                            method, path, query, body, prefer, &new_token, true,
                        );
                    }
                    Err(err) => {
                        // The stored refresh token is dead — surface it so
                        // the caller re-prompts.
                        if matches!(err, AppError::InvalidGrant) {
                            return Err(err);
                        }
                    }
                }
            }
        }
        Ok(resp)
    }

    fn call(
        &self,
        method: reqwest::Method,
        path: &str,
        query: &[(String, String)],
        body: Option<Value>,
        prefer: Option<&str>,
    ) -> Result<Value> {
        let resp = self.response(method, path, query, body, prefer)?;
        let status = resp.status();
        let text = resp.text().map_err(AppError::Network)?;
        if !status.is_success() {
            return Err(map_postgrest_error(status, &text));
        }
        serde_json::from_str(&text).map_err(|_| AppError::Remote(format!("bad JSON from {path}")))
    }

    /// For endpoint types that only need a success/failure (DELETE, and
    /// upserts where the response body is irrelevant).
    fn call_unit(
        &self,
        method: reqwest::Method,
        path: &str,
        query: &[(String, String)],
        body: Option<Value>,
        prefer: Option<&str>,
    ) -> Result<()> {
        let resp = self.response(method, path, query, body, prefer)?;
        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().map_err(AppError::Network)?;
            return Err(map_postgrest_error(status, &text));
        }
        Ok(())
    }

    fn entries_query(f: &EntryFilter) -> Vec<(String, String)> {
        let mut q = vec![(
            "select".to_string(),
            "id,date,kind,amount,from_account_id,to_account_id,concept,subconcept,description,from_account,to_account"
                .to_string(),
        )];
        if let Some(period) = &f.period {
            let lo = period.start();
            let hi = period.end_exclusive();
            // `and`, not `or`: an `or` of the two bounds matches every date.
            q.push((
                "and".to_string(),
                format!("(date.gte.{lo},date.lt.{hi})"),
            ));
        }
        if let Some(up_to) = &f.up_to_date {
            q.push(("date".to_string(), format!("lte.{up_to}")));
        }
        if let Some(kind) = f.kind {
            q.push(("kind".to_string(), format!("eq.{}", kind.as_str())));
        }
        if let Some(concept) = &f.concept {
            q.push(("concept".to_string(), format!("eq.{concept}")));
        }
        if let Some(account_id) = f.account_id {
            q.push((
                "or".to_string(),
                format!("(from_account_id.eq.{account_id},to_account_id.eq.{account_id})"),
            ));
        }
        q.push(("order".to_string(), "date.desc,id.desc".to_string()));
        q
    }

    /// GET every row of a list endpoint, page by page (see [`paginate`]).
    /// `q` must include a stable `order`.
    fn get_all(&self, path: &str, q: &[(String, String)], limit: Option<usize>) -> Result<Vec<Value>> {
        paginate(limit, |offset, count| {
            let mut page_q = q.to_vec();
            page_q.push(("limit".to_string(), count.to_string()));
            page_q.push(("offset".to_string(), offset.to_string()));
            let v = self.call(reqwest::Method::GET, path, &page_q, None, None)?;
            Ok(v.as_array().cloned().unwrap_or_default())
        })
    }

    fn entry_from_json(v: &Value) -> Entry {
        let kind_str = v["kind"].as_str().unwrap_or("expense");
        let id = v["entry_id"].as_i64().or_else(|| v["id"].as_i64()).unwrap_or(0);
        Entry {
            id,
            date: v["date"].as_str().unwrap_or("").to_string(),
            kind: EntryKind::from_str(kind_str).unwrap_or(EntryKind::Expense),
            amount: v["amount"].as_f64().unwrap_or(0.0),
            from_account_id: v["from_account_id"].as_i64(),
            to_account_id: v["to_account_id"].as_i64(),
            from_account: v["from_account"].as_str().map(|s| s.to_string()),
            to_account: v["to_account"].as_str().map(|s| s.to_string()),
            concept: v["concept"].as_str().map(|s| s.to_string()),
            subconcept: v["subconcept"].as_str().map(|s| s.to_string()),
            description: v["description"].as_str().map(|s| s.to_string()),
        }
    }

    fn account_from_json(v: &Value) -> Account {
        Account {
            id: v["id"].as_i64().unwrap_or(0),
            name: v["name"].as_str().unwrap_or("").to_string(),
            kind: AccountKind::from_str(v["kind"].as_str().unwrap_or("spending"))
                .unwrap_or(AccountKind::Spending),
            target_amount: v["target_amount"].as_f64(),
            credit_limit: v["credit_limit"].as_f64(),
            liquid: v["liquid"].as_bool().unwrap_or(true),
            archived: v["archived"].as_bool().unwrap_or(false),
        }
    }

    fn to_new_entry_json(e: &NewEntry) -> Value {
        serde_json::json!({
            "date": e.date,
            "kind": e.kind.as_str(),
            "amount": e.amount,
            "from_account_id": e.from_account_id,
            "to_account_id": e.to_account_id,
            "concept": e.concept,
            "subconcept": e.subconcept,
            "description": e.description,
        })
    }
}

/// PostgREST caps every response (1000 rows by default on Supabase) and truncates
/// silently, so lists are read in pages until a short page arrives or `limit`
/// is reached. `fetch(offset, count)` returns one page.
const PAGE_SIZE: usize = 1000;

fn paginate(
    limit: Option<usize>,
    mut fetch: impl FnMut(usize, usize) -> Result<Vec<Value>>,
) -> Result<Vec<Value>> {
    let mut out = Vec::new();
    loop {
        let remaining = limit.map_or(PAGE_SIZE, |l| l - out.len());
        let want = remaining.min(PAGE_SIZE);
        if want == 0 {
            break;
        }
        let page = fetch(out.len(), want)?;
        let got = page.len();
        out.extend(page);
        if got < want {
            break;
        }
    }
    Ok(out)
}

fn map_postgrest_error(status: reqwest::StatusCode, text: &str) -> AppError {
    let default = |s: &str| AppError::Remote(format!("HTTP {status}: {s}"));
    match serde_json::from_str::<Value>(text) {
        Ok(v) => {
            if let Some(code) = v["code"].as_str() {
                let message = v["message"].as_str().unwrap_or("").to_string();
                return match code {
                    "23505" => AppError::Invalid(format!("Duplicate record: {message}")),
                    "23514" => AppError::Invalid(message.to_string()),
                    "23503" => AppError::Invalid(format!("Referenced record missing: {message}")),
                    "42501" => AppError::Auth(format!("Permission denied: {message}")),
                    "42P01" | "42P02" | "42804" => {
                        AppError::Remote(format!("Schema not migrated on Supabase ({code}): {message}"))
                    }
                    "PGRST116" => AppError::NotFound("Record not found".into()),
                    _ => AppError::Remote(format!("{code}: {message}")),
                };
            }
            if let Some(msg) = v["message"].as_str() {
                return AppError::Remote(msg.to_string());
            }
            default(text)
        }
        Err(_) => default(text),
    }
}

impl LedgerBackend for SupabaseBackend {
    fn raw_accounts(&self, include_archived: bool) -> Result<Vec<Account>> {
        let mut q = vec![
            ("select".to_string(), "id,name,kind,target_amount,credit_limit,liquid,archived".to_string()),
            ("order".to_string(), "kind,name".to_string()),
        ];
        if !include_archived {
            q.push(("archived".to_string(), "eq.false".to_string()));
        }
        Ok(self
            .get_all("/rest/v1/accounts", &q, None)?
            .iter()
            .map(Self::account_from_json)
            .collect())
    }

    fn find_account_by_name(&self, name: &str) -> Result<Option<AccountBalance>> {
        let q = vec![
            ("select".to_string(), "id,name,kind,target_amount,credit_limit,liquid,archived".to_string()),
            ("name".to_string(), format!("eq.{name}")),
        ];
        let v = self.call(reqwest::Method::GET, "/rest/v1/accounts", &q, None, None)?;
        let arr = v.as_array();
        if arr.is_none_or(|a| a.is_empty()) {
            return Ok(None);
        }
        let account = Self::account_from_json(&arr.unwrap()[0]);
        // Need entries to derive balance
        let entries = self.entries(&EntryFilter::default())?;
        Ok(crate::storage::ledger::derive_balances(&[account], &entries)
            .into_iter()
            .next())
    }

    fn entries(&self, f: &EntryFilter) -> Result<Vec<Entry>> {
        let limit = f.limit.map(|l| l as usize);
        Ok(self
            .get_all("/rest/v1/entries_view", &Self::entries_query(f), limit)?
            .iter()
            .map(Self::entry_from_json)
            .collect())
    }

    fn insert_account(&self, new: &NewAccount) -> Result<i64> {
        let body = serde_json::json!({
            "name": new.name,
            "kind": new.kind.as_str(),
            "target_amount": new.target_amount,
            "credit_limit": new.credit_limit,
            "liquid": new.liquid,
        });
        let v = self.call(
            reqwest::Method::POST,
            "/rest/v1/accounts",
            &[],
            Some(body),
            Some("return=representation"),
        )?;
        let arr = v.as_array().ok_or_else(|| AppError::Remote("accounts insert: no row".into()))?;
        let row = arr.first().ok_or_else(|| AppError::Remote("accounts insert: empty response".into()))?;
        row["id"].as_i64().ok_or_else(|| AppError::Remote("accounts insert: no id".into()))
    }

    fn set_archived(&self, id: i64) -> Result<()> {
        let q = vec![("id".to_string(), format!("eq.{id}"))];
        self.call_unit(
            reqwest::Method::PATCH,
            "/rest/v1/accounts",
            &q,
            Some(serde_json::json!({ "archived": true })),
            None,
        )
    }

    fn push_entries(&self, entries: &[NewEntry]) -> Result<Vec<Entry>> {
        let payload: Vec<Value> = entries.iter().map(Self::to_new_entry_json).collect();
        let v = self.call(
            reqwest::Method::POST,
            "/rest/v1/rpc/apply_entries",
            &[],
            Some(serde_json::json!({ "p_entries": payload })),
            None,
        )?;
        let arr = v.as_array().ok_or_else(|| AppError::Remote("apply_entries: bad response".into()))?;
        Ok(arr.iter().map(Self::entry_from_json).collect())
    }

    /// `upd` arrives already validated and merged by `rules::merge_entry_update`
    /// (via `entry_service::update`), so this only writes it.
    fn update_entry(&self, id: i64, upd: &EntryUpdate) -> Result<Entry> {
        let q = vec![("id".to_string(), format!("eq.{id}"))];
        let body = serde_json::json!({
            "date": upd.date,
            "amount": upd.amount,
            "from_account_id": upd.from_account_id,
            "to_account_id": upd.to_account_id,
            "concept": upd.concept,
            "subconcept": upd.subconcept,
            "description": upd.description,
        });
        self.call_unit(reqwest::Method::PATCH, "/rest/v1/entries", &q, Some(body), None)?;
        self.get_entry(id)
    }

    fn delete_entry(&self, id: i64) -> Result<()> {
        let q = vec![("id".to_string(), format!("eq.{id}"))];
        self.call_unit(reqwest::Method::DELETE, "/rest/v1/entries", &q, None, None)
    }

    fn get_entry(&self, id: i64) -> Result<Entry> {
        let q = vec![
            ("select".to_string(), "id,date,kind,amount,from_account_id,to_account_id,concept,subconcept,description,from_account,to_account".to_string()),
            ("id".to_string(), format!("eq.{id}")),
        ];
        let v = self.call(reqwest::Method::GET, "/rest/v1/entries_view", &q, None, None)?;
        let arr = v.as_array().ok_or_else(|| AppError::Remote("entries_view: bad response".into()))?;
        arr.first()
            .map(Self::entry_from_json)
            .ok_or_else(|| AppError::NotFound(format!("Entry #{id} not found")))
    }

    fn get_config(&self, key: &str) -> Result<Option<String>> {
        let q = vec![
            ("select".to_string(), "value".to_string()),
            ("key".to_string(), format!("eq.{key}")),
        ];
        let v = self.call(reqwest::Method::GET, "/rest/v1/config", &q, None, None)?;
        Ok(v.as_array()
            .and_then(|a| a.first())
            .and_then(|r| r["value"].as_str().map(|s| s.to_string())))
    }

    fn set_config(&self, key: &str, value: &str) -> Result<()> {
        let body = serde_json::json!({ "key": key, "value": value });
        self.call_unit(
            reqwest::Method::POST,
            "/rest/v1/config",
            &[],
            Some(body),
            Some("resolution=merge-duplicates"),
        )
    }

    fn list_config(&self) -> Result<Vec<Config>> {
        let q = vec![
            ("select".to_string(), "key,value".to_string()),
            ("order".to_string(), "key".to_string()),
        ];
        Ok(self
            .get_all("/rest/v1/config", &q, None)?
            .iter()
            .map(|r| Config {
                key: r["key"].as_str().unwrap_or("").to_string(),
                value: r["value"].as_str().unwrap_or("").to_string(),
            })
            .collect())
    }

    fn list_concepts(&self, type_filter: Option<&str>) -> Result<Vec<Concept>> {
        let mut q = vec![
            ("select".to_string(), "id,name,concept_type".to_string()),
            ("order".to_string(), "name".to_string()),
        ];
        if let Some(t) = type_filter {
            q.push(("or".to_string(), format!("(concept_type.eq.{t},concept_type.eq.both)")));
        }
        Ok(self
            .get_all("/rest/v1/concepts", &q, None)?
            .iter()
            .map(|r| Concept {
                id: r["id"].as_i64(),
                name: r["name"].as_str().unwrap_or("").to_string(),
                concept_type: r["concept_type"].as_str().unwrap_or("").to_string(),
            })
            .collect())
    }

    fn add_concept(&self, name: &str, concept_type: &str) -> Result<()> {
        let body = serde_json::json!({ "name": name, "concept_type": concept_type });
        self.call_unit(reqwest::Method::POST, "/rest/v1/concepts", &[], Some(body), None)
    }

    fn set_budget(&self, concept: &str, limit: f64, period: &str) -> Result<()> {
        let body = serde_json::json!({
            "concept": concept,
            "monthly_limit": limit,
            "period": period,
        });
        self.call_unit(
            reqwest::Method::POST,
            "/rest/v1/budgets",
            &[],
            Some(body),
            Some("resolution=merge-duplicates"),
        )
    }

    fn list_budgets(&self, period: Option<&str>) -> Result<Vec<Budget>> {
        let mut q = vec![
            ("select".to_string(), "id,concept,monthly_limit,period".to_string()),
            ("order".to_string(), "concept,period".to_string()),
        ];
        if let Some(p) = period {
            q.push(("period".to_string(), format!("eq.{p}")));
        }
        Ok(self
            .get_all("/rest/v1/budgets", &q, None)?
            .iter()
            .map(|r| Budget {
                id: r["id"].as_i64(),
                concept: r["concept"].as_str().unwrap_or("").to_string(),
                monthly_limit: r["monthly_limit"].as_f64().unwrap_or(0.0),
                period: r["period"].as_str().unwrap_or("").to_string(),
            })
            .collect())
    }

    fn delete_budget(&self, concept: &str, period: &str) -> Result<()> {
        let q = vec![
            ("concept".to_string(), format!("eq.{concept}")),
            ("period".to_string(), format!("eq.{period}")),
        ];
        self.call_unit(reqwest::Method::DELETE, "/rest/v1/budgets", &q, None, None)
    }

    fn status(&self) -> Result<crate::models::LedgerStatus> {
        match self.call(
            reqwest::Method::POST,
            "/rest/v1/rpc/ledger_status",
            &[],
            Some(serde_json::json!({})),
            None,
        ) {
            Ok(v) => Ok(crate::models::LedgerStatus {
                revision: v["revision"].as_i64().unwrap_or(0),
                schema_version: v["schema_version"].as_i64().unwrap_or(1),
            }),
            // Before 0002 the RPC doesn't exist: that *is* schema version 1.
            Err(AppError::Remote(m)) if m.starts_with("PGRST202") => Ok(crate::models::LedgerStatus {
                revision: self.remote_revision()?,
                schema_version: 1,
            }),
            Err(e) => Err(e),
        }
    }

    fn export_snapshot(&self) -> Result<crate::models::LedgerSnapshot> {
        let v = self.call(
            reqwest::Method::POST,
            "/rest/v1/rpc/export_ledger",
            &[],
            Some(serde_json::json!({})),
            None,
        )?;
        let arr = |k: &str| v[k].as_array().cloned().unwrap_or_default();
        Ok(crate::models::LedgerSnapshot {
            revision: v["revision"].as_i64().unwrap_or(0),
            schema_version: v["schema_version"].as_i64().unwrap_or(0),
            exported_at: v["exported_at"].as_str().unwrap_or("").to_string(),
            concepts: arr("concepts")
                .iter()
                .map(|r| Concept {
                    id: r["id"].as_i64(),
                    name: r["name"].as_str().unwrap_or("").to_string(),
                    concept_type: r["concept_type"].as_str().unwrap_or("").to_string(),
                })
                .collect(),
            accounts: arr("accounts").iter().map(Self::account_from_json).collect(),
            entries: arr("entries").iter().map(Self::entry_from_json).collect(),
            budgets: arr("budgets")
                .iter()
                .map(|r| Budget {
                    id: r["id"].as_i64(),
                    concept: r["concept"].as_str().unwrap_or("").to_string(),
                    monthly_limit: r["monthly_limit"].as_f64().unwrap_or(0.0),
                    period: r["period"].as_str().unwrap_or("").to_string(),
                })
                .collect(),
            config: arr("config")
                .iter()
                .map(|r| Config {
                    key: r["key"].as_str().unwrap_or("").to_string(),
                    value: r["value"].as_str().unwrap_or("").to_string(),
                })
                .collect(),
        })
    }

    fn session_email(&self) -> Option<String> {
        self.session
            .lock()
            .unwrap()
            .as_ref()
            .and_then(|s| s.user.as_ref())
            .and_then(|u| u.email.clone())
    }
}

impl std::fmt::Debug for SupabaseBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SupabaseBackend({})", self.base_url)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::period::Period;

    #[test]
    fn period_filter_is_an_and_range_that_coexists_with_the_account_filter() {
        let f = EntryFilter {
            period: Some(Period::parse("2026-08").unwrap()),
            account_id: Some(3),
            ..Default::default()
        };
        let q = SupabaseBackend::entries_query(&f);
        assert!(q.contains(&("and".to_string(), "(date.gte.2026-08-01,date.lt.2026-09-01)".to_string())));
        assert!(q.contains(&("or".to_string(), "(from_account_id.eq.3,to_account_id.eq.3)".to_string())));
        assert_eq!(q.iter().filter(|(k, _)| k == "or").count(), 1);
        assert!(!q.iter().any(|(k, _)| k == "limit"), "limit is applied by paginate()");
    }

    #[test]
    fn paginate_keeps_fetching_full_pages_and_stops_on_a_short_one() {
        let total = 2037usize;
        let mut calls = Vec::new();
        let rows = paginate(None, |offset, count| {
            calls.push((offset, count));
            let n = count.min(total.saturating_sub(offset));
            Ok(vec![Value::Null; n])
        })
        .unwrap();
        assert_eq!(rows.len(), total);
        assert_eq!(calls, vec![(0, 1000), (1000, 1000), (2000, 1000)]);
    }

    #[test]
    fn paginate_honors_a_caller_limit() {
        let mut calls = Vec::new();
        let rows = paginate(Some(1500), |offset, count| {
            calls.push((offset, count));
            Ok(vec![Value::Null; count])
        })
        .unwrap();
        assert_eq!(rows.len(), 1500);
        assert_eq!(calls, vec![(0, 1000), (1000, 500)]);
    }
}
