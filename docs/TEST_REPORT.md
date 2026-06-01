# Test Report — Amway CCS Tracker

**Tester role.** Verifies the implementation compiles, the business rules are
enforced in code (not just the UI), and the automated suite passes.

## 1. Compile check

| Command | Result |
|---------|--------|
| `cargo build` (debug) | ✅ Finished, **0 warnings, 0 errors** |
| `cargo build --release --target x86_64-pc-windows-msvc` | ✅ Finished — `target\x86_64-pc-windows-msvc\release\amway_ccs_tracker.exe` (≈6.1 MB) |
| `cargo test` | ✅ **27 passed; 0 failed** |

Dependency versions match the spec: `eframe`/`egui` 0.28, `rusqlite` 0.31
(bundled), `chrono` 0.4, `serde`/`serde_json` 1, `thiserror` 1; plus
`egui_extras` 0.28 (`TableBuilder` for full-width tables) and `png` 0.17
(network-chart PNG export).

## 2. Automated tests written

### Unit tests — `src/utils/scoring.rs`
| Test | Asserts |
|------|---------|
| `prospect_total_is_sum_of_all_fields` | ProspectScore total = sum of 5 fields |
| `customer_total_is_sum_of_all_fields` | CustomerScore total = sum of 3 fields |
| `prospect_field_out_of_range_is_rejected` | field > max / < min → `Err` |
| `customer_field_out_of_range_is_rejected` | field > max / < min → `Err` |
| `rank_progression_thresholds` | 5000 PV = C1, 10k=CL, 20k=CL15, 30k=CL21 |
| `bonus_percent_tiers` | 6/9/12/15/18/21% tier boundaries |
| `rank_cannot_regress` | CL→C1 `Err`; KOC→C1, hold, advance `Ok` |
| `qualified_rank_needs_both_ppv_and_legs` | rank advisor: PPV + 3-leg conditions per "5 Steps to 21%" |
| `sponsor_step_must_advance_sequentially` | Step1→Step5 `Err`; +1 / back / hold `Ok` |

### DB integration tests — `src/db/queries.rs` (in-memory SQLite)
| Test | Asserts |
|------|---------|
| `insert_then_read_back_matches` | insert → read back, fields match |
| `update_sponsor_step_persists` | step advance persists + date recorded |
| `follow_up_checkbox_toggle_persists` | toggle → save → reload retains state |
| `delete_cascades_to_scores_and_follow_up` | delete contact cascades dependents |
| `sponsor_must_reference_an_abo` | sponsor=Prospect/ghost `Err`; sponsor=ABO `Ok` |
| `prospect_score_out_of_range_is_rejected` | relationship=11 `Err` |
| `sponsor_step_cannot_skip` | guided advance: set Step5 from Step1 `Err` |
| `sponsor_step_direct_allows_jumps_for_manual_edit` | manual edit: jump to Step6 / back to Step2 `Ok`, dates kept |
| `rank_cannot_regress_on_update` | CL→C1 `Err`; CL→CL21 `Ok` |
| `changing_type_drops_opposing_score` | Prospect→Customer clears prospect score |
| `abo_rows_resolve_upline_name_and_filter_by_type` | ABO list shows only ABOs + resolves upline name; search filters |
| `abo_leg_counts_and_ppv_round_trip` | counts C1+/CL+/CL15+ direct legs; PPV persists |
| `me_leg_counts_and_ppv_round_trip` | self rank: counts my direct legs (sponsor = me / NULL), excludes deeper ABOs; my PPV round-trips through the `meta` store |
| `list_all_activities_joins_contacts_and_filters` | aggregate history: joins activities to their contact, newest first, filters by contact name and by note text |
| `activity_kinds_crud_and_rename_relabels_activities` | activity types CRUD: add (rejects blank/duplicate); rename relabels existing activities; delete keeps past activities' text |
| `customer_rows_resolve_upline_name` | customer list resolves the managing-upline ABO name; `None` (mine) when no sponsor is set |
| `member_abo_numbers_round_trip` | optional member_no / abo_no persist through insert and update (set, clear, change) |
| `activities_add_list_delete_and_cascade` | activity log: add/list (newest first)/delete; cascades on contact delete |

## 3. Business-rule enforcement (verified in code, not just UI)

| Rule | Enforced at |
|------|-------------|
| A person cannot be both Prospect AND Customer | `contact_type` enum + opposing-score drop in `update_contact` ([queries.rs:136](../src/db/queries.rs)) + type check in score upserts |
| `sponsor_id` must reference an ABO (and not self) | `ensure_sponsor_valid` ([queries.rs:81](../src/db/queries.rs)), called by insert/update |
| Prospect score ranges (1–10 / 1–5) | `validate_prospect_fields` ([scoring.rs:28](../src/utils/scoring.rs)) in `upsert_prospect_score` ([queries.rs:233](../src/db/queries.rs)) |
| Customer score ranges | `validate_customer_fields` ([scoring.rs:61](../src/utils/scoring.rs)) |
| Sponsor step advances sequentially | `validate_step_transition` ([scoring.rs:121](../src/utils/scoring.rs)) in `set_sponsor_step` ([queries.rs:399](../src/db/queries.rs)) |
| Rank can only advance, not regress | `validate_rank_transition` ([scoring.rs:106](../src/utils/scoring.rs)) in `update_contact` |
| Delete cascades to scores/follow-up | FK `ON DELETE CASCADE` + `PRAGMA foreign_keys=ON` ([schema.rs](../src/db/schema.rs), [db/mod.rs](../src/db/mod.rs)) |

## 4. Error handling review

* No `unwrap()` / `expect()` on production paths — all fallible ops return
  `Result<T, AppError>`; `unwrap()` appears only inside `#[cfg(test)]`.
* All DB access goes through the single `DbConnection` wrapper — no global/static
  connections.
* `AppError` (`thiserror`) wraps rusqlite/io/serde errors and carries validation
  messages; the UI surfaces them in a dismissible status bar via
  `AppState::set_error`.

## 5. Manual QA checklist

Build & launch: `cargo run` (or run the release `.exe`). Use Settings →
*Load sample data* to populate the views.

- [ ] App launches without crash; Thai text renders (not boxes)
- [ ] Add new prospect → appears in Prospects list
- [ ] Score total updates live as score fields change in the form
- [ ] Sponsor step dropdown sets any step (jump/back) for editing; ▶ still advances one step at a time
- [ ] Advancing past Step 8 shows "last step" message (no crash)
- [ ] Add customer → appears in Customers list, sorted by score
- [ ] Customer upline: the add/edit form has a searchable อัพไลน์ (Sponsor) combo; choosing a downline ABO shows it in the Customers list's อัพไลน์ column (else "ฉัน (ME)")
- [ ] Customer form has an optional เลข Member field; ABO form has an optional เลข ABO field; leaving blank saves fine; values persist after reopening (absent on Prospects)
- [ ] ABO page lists business partners with rank + upline; add/edit/delete works
- [ ] ABO 📊 Rank Advisor: edit PPV, see leg counts + qualified rank + condition checklist; ▲ applies an advancing rank
- [ ] Network → 📊 ประเมินระดับของฉัน (ME): edit my PPV, see my direct-leg counts + qualified rank + checklist; my PPV persists; the central node shows my computed rank
- [ ] 📝 Activity Log (any table): add an entry (kind + note), it appears newest-first; delete works; entries survive app restart
- [ ] ประวัติติดต่อ (Activity History) menu: lists every entry across all contacts newest-first; search by name/note and the kind filter narrow the list; 📝 opens that contact's log; 🗑 removes an entry
- [ ] ประเภทกิจกรรม (Activity Types) menu: add a type; rename it (existing history relabels); delete it (🗑 confirms; history keeps its text); blank/duplicate names are rejected; the new type appears in the activity-log dropdown
- [ ] Follow-Up ABO picker & ABO form's upline selector: typing in the combo filters the list; selecting still works; "ฉัน (ME)" stays available for the sponsor
- [ ] Follow-up checkboxes persist after closing & reopening the app
- [ ] Follow-up progress bar updates as items are checked
- [ ] Network chart renders the seeded hierarchy radially (ฉัน → พิชัย → สมหญิง → วีระ)
- [ ] Network → 💾 บันทึกรูป writes a PNG of the visible chart to `%APPDATA%\AmwayCCSTracker\exports\` and shows the saved path in the status bar
- [ ] Clicking a table column header sorts it; clicking again flips direction (▲/▼)
- [ ] Search box filters prospect/customer lists by name/phone in real time
- [ ] Edit contact → changes saved and reflected in the list
- [ ] Delete (🗑) on any table → confirm modal appears; Cancel keeps the row, ลบ removes it from all views with scores/follow-up gone
- [ ] Settings calculator honours conditions: 15000 PV + 0 legs → C1 (9% bonus); 15000 PV + 3 C1-legs → CL
- [ ] Try to set an ABO's sponsor to itself / a prospect → rejected with a message

## 6. Notes / engineering decisions (flagged to Lead)

* **Thai labels in the spec were OCR-garbled** (`ความมั่ยั่น`, `คันหา`, `คู้ค้า`,
  `พฤกษิน`) and were corrected to standard Thai.
* **Delete semantics**: scores/follow-up cascade, but a deleted sponsor sets the
  downline's `sponsor_id` to NULL rather than deleting the downline — preserving
  records is the safer business behaviour.
* **Prospect-score max is 30** (10+5+5+5+5). The spec's "max 20" reflected a
  different field weighting; the implementation uses the field ranges as written.
* **Versions honored as pinned** (egui 0.28 / rusqlite 0.31); both build cleanly
  on the Rust 1.95 / MSVC 2026 toolchain present.
* **PV → rank/bonus logic verified** against the spec: rank thresholds
  (5,000=C1 / 10,000=CL / 20,000=CL15 / 30,000=CL21) and bonus tiers
  (5k=6% … 150k=21%). `bonus_percent_tiers` now also asserts one-unit-below each
  threshold to rule out off-by-one. Bonus % uses the 6-tier PV table; **rank now
  honours the full conditions** (PPV threshold AND 3 downline legs at the prior
  rank) via `qualified_rank`, used by both the ABO Rank Advisor and the Settings
  calculator. So PV alone reaches at most C1 — e.g. 15,000 PV with no qualifying
  legs is C1 (bonus 9%); CL needs 15,000≥10,000 PV **plus** 3 C1+ legs. No 3%
  entry-tier is defined in the spec (real Amway has one below 5,000 PV).
