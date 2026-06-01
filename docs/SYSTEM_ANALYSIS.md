# System Analysis — Amway CCS Tracker

Domain model for the CCS Guide (Crown Center Success) prospect & downline
tracker. This is the specification the implementation follows.

## A. Entity-Relationship Diagram

```
                         ┌───────────────────────────┐
                         │          contacts          │
                         │───────────────────────────│
                         │ id            PK           │
                         │ name, nickname, phone,     │
                         │ line_id, age, gender,      │
                         │ address, network_category  │
                         │ contact_type  (P/C/ABO)    │
                         │ rank          (ABO only)   │
            ┌────────────│ sponsor_id    FK ─────────┐│  self-reference
            │  upline    │ created_at, notes         ││  (ABO → ABO)
            │            └───────────────────────────┘│
            └─────────────────────────────────────────┘
                  │ 1            │ 1            │ 1            │ 1
                  │              │              │              │
        (type=Prospect) (type=Customer)  (type=Prospect)  (type=ABO)
                  │ 0..1         │ 0..1         │ 0..1         │ 0..1
        ┌─────────▼──────┐ ┌─────▼─────────┐ ┌──▼──────────────┐ ┌▼──────────────────┐
        │ prospect_scores│ │customer_scores│ │sponsor_flow_    │ │ follow_up_sheets  │
        │ contact_id PK/FK│ │contact_id PK/FK│ │status           │ │ contact_id PK/FK  │
        │ relationship..  │ │relationship.. │ │ contact_id PK/FK│ │ 26 bool items     │
        │ financial..     │ │financial..    │ │ current_step    │ │ (BK1/BK2/C1/Conf) │
        │ leadership..    │ │decision_power │ │ step_date JSON  │ │ updated_at        │
        │ total (derived) │ │problems, total│ │ notes           │ │                   │
        └─────────────────┘ └───────────────┘ └─────────────────┘ └───────────────────┘

Delete semantics: deleting a contact CASCADEs its score/flow/follow-up rows.
A deleted sponsor sets its downline's sponsor_id to NULL (downline preserved).
```

## B. Entities & fields

### contacts — unified person record
| Field | Type | Notes |
|-------|------|-------|
| id | INTEGER PK | autoincrement |
| name | TEXT (required) | full name |
| nickname | TEXT? | required via the add/edit form; used as the node label in the network chart (column stays nullable for legacy rows) |
| phone / line_id / address | TEXT? | optional |
| age | INTEGER? | 0–255 |
| gender | TEXT | `Male` / `Female` |
| network_category | TEXT | Family/Relative/Friend/Coworker/Partner/Acquaintance/Stranger |
| contact_type | TEXT | `Prospect` / `Customer` / `ABO` |
| rank | TEXT? | ABO only: `KOC`/`C1`/`CL`/`CL15`/`CL21` |
| sponsor_id | INTEGER? FK→contacts | upline ABO (set for ABOs and Customers; the target must be an ABO) |
| created_at | TEXT | RFC3339 |
| notes | TEXT? | |
| ppv | INTEGER | Personal Point Value (ABO rank qualification); added in schema v2 |

### prospect_scores — Sponsor List scoring (1:1 with a Prospect)
`relationship_closeness` (1–10), `financial_stability`, `leadership`,
`financial_status`, `accessibility` (each 1–5), `total` (derived, max 30).

### customer_scores — Customer Name List scoring (1:1 with a Customer)
`relationship_level` (1–10), `financial_status`, `decision_power` (each 1–5),
`problems` (text), `total` (derived, max 20).

### sponsor_flow_status — position in the 8-step flow (1:1 with a Prospect)
`current_step` (1–8), `step_date` (JSON map step→date), `notes`.

| Step | Thai |
|------|------|
| 1 | จดรายชื่อ เช็คฟอร์มเบื้องต้น |
| 2 | สร้างนัด |
| 3 | เช็คฟอร์มหน้างาน ค้นหาความต้องการ |
| 4 | เปิดใจ ชวนคิด |
| 5 | เปิดภาพ สินค้า/ธุรกิจ |
| 6 | ปิดการสมัคร |
| 7 | นัดหมายติดตาม BK / พบอัพไลน์ |
| 8 | วิเคราะห์ ออกแบบ วางแผน |

### follow_up_sheets — BK1/BK2/C1/Conference checklist (1:1 with an ABO)
26 booleans + `updated_at`: BK1 (9), BK2 (7), C1 Qualification (7),
CCS Conference (3).

### activities — interaction history (many per contact, schema v3)
`id`, `contact_id` (FK→contacts, ON DELETE CASCADE), `kind` (the activity-type
*name*, stored as text — see `activity_kinds`), `note` (free text), `created_at`.
Logs what was done with a prospect/customer/ABO (สาธิตสินค้า, บอกโปรโมชั่น, พูดแผน, …).

### activity_kinds — user-managed activity types (schema v5)
`id`, `name` (UNIQUE). The list of types shown in the activity-log dropdown and
the history filter, editable via the Activity Types screen. Renaming a type also
relabels matching `activities.kind`; deleting one leaves past activities' text
intact (it just disappears from the dropdown). Seeded with the former built-in
kinds on migration. Activities store the name (not an FK id) so history is
self-contained.

### meta — app-level key/value store (schema v4)
`key` (PK), `value`. Holds settings that have no contact row — notably
`me_ppv`, my own Personal PV, used to assess *my* ("ฉัน / ME") rank. "Me" is the
implicit network root, so my direct downline are the ABOs with `sponsor_id`
NULL, and my rank is derived the same way an ABO's is.

### Rank progression & PV tiers (reference logic, in `utils/scoring.rs`)
* Rank from Personal Group PV: `<5,000`→KOC, `5,000`→C1, `10,000`→CL,
  `20,000`→CL15, `30,000`→CL21.
* Bonus % tiers: 5,000=6%, 15,000=9%, 30,000=12%, 55,000=15%, 90,000=18%,
  150,000=21%.
* Rank qualification (Rank Advisor): besides PPV, a rank needs 3 direct downline
  legs at the prior rank — CL needs PPV ≥ 10,000 + 3×(C1 or above); CL15 needs
  ≥ 20,000 + 3×(CL+); CL21 needs ≥ 30,000 + 3×(CL15+). C1 needs only PPV ≥ 5,000.

## C. Business rules

1. **Type exclusivity** — `contact_type` is a single enum; a person is exactly
   one of Prospect / Customer / ABO. Switching type drops the opposing score row.
2. **Sponsor integrity** — a non-null `sponsor_id` must reference an existing
   **ABO**, and a contact cannot sponsor itself.
3. **Score ranges** — relationship 1–10; all other score fields 1–5;
   out-of-range input is rejected. `total` is always recomputed server-side.
4. **Sequential sponsor flow** — the flow advances one step at a time; skipping
   ahead (e.g. Step 1 → Step 5) is rejected. Moving back to correct a mistake is
   allowed. Reaching a step records its date.
5. **Monotonic rank** — rank may advance or hold, never regress.
6. **Cascade vs. preserve** — deleting a contact cascades its dependent rows;
   deleting a sponsor preserves the downline (their `sponsor_id` becomes NULL).
7. **Local only** — all data in a local SQLite file; no network calls.

## D. Screens

| Screen | Purpose |
|--------|---------|
| Dashboard | Cards: prospects, customers, ABOs, this-month conversions; customer 20-target bar; sponsor-flow overview |
| Prospects | Sponsor List table, sortable columns, inline editable step (dropdown sets any step) + ▶ advance, search, add/edit/delete |
| Customers | Customer Name List table, sortable columns (incl. upline), search, add/edit/delete; the form has a searchable upline (Sponsor) selector so a VIP customer can be assigned to a downline ABO we manage (or "ฉัน (ME)" = ours directly) |
| ABO | Business-partner management table (sortable: name/phone/rank/upline), search, add/edit/delete, + Rank Advisor (📊) computing the qualified rank from PPV + downline legs. The add/edit form's upline (sponsor) selector is a searchable combo |
| Follow Up | Per-ABO BK1/BK2/C1/Conference checklist with completion progress bar; the ABO picker is a searchable combo |
| Network | Radial node chart — "me" at the centre (showing my own qualified rank), downline radiating out, straight-line links; nodes are draggable, with zoom in/out controls and an Auto-arrange button that resets the layout, drag offsets, and zoom (back to 100%); a 📊 button opens my self Rank Advisor (my direct legs + my PPV → my qualified rank); 💾 บันทึกรูป exports the visible chart as a PNG |
| Activity Log | Per-contact interaction-history modal (📝 from any list): add/view/delete entries by kind + free note |
| Activity History | Aggregate timeline of every logged interaction across all contacts, newest first; text search (name/note) + kind filter; jump to a contact's log (📝) or delete an entry (🗑) |
| Activity Types | Manage the activity-type list (CRUD): add, rename (relabels existing activities), delete (past activities keep their text). Feeds the activity-log dropdown and the history filter |
| Settings | DB location, font, total contacts, sample-data seeder, rank/bonus calculator (PV + downline-leg counts → qualified rank, matching the full conditions) |

## E. Data flow — how a person moves through the pipeline

```
        ┌─────────────┐  score (Sponsor List)        advance steps 1→8
  add → │  PROSPECT   │ ───────────────────────────────────────────────┐
        └─────────────┘                                                 │
              │ qualifies, registers (Step 6 ปิดสมัคร)                   │
              ▼                                                          │
        ┌─────────────┐  follow-up BK1→BK2→C1   rank KOC→C1→CL→CL15→CL21│
        │     ABO     │ ───────────────────────────────────────────────┘
        └─────────────┘  becomes a sponsor → appears in Network tree
              ▲
              │  (alternatively) a contact may instead be tracked as…
        ┌─────────────┐  score (Customer List), target 20 customers
  add → │  CUSTOMER   │
        └─────────────┘
```

A Prospect is worked through the 8-step sponsor flow; on registration they
become an ABO with a follow-up sheet and a rank that climbs the 5-steps-to-21%
ladder. Independently, people can be tracked as VIP Customers with their own
scoring toward the 20-customer goal. Type changes are mutually exclusive and the
app clears the now-irrelevant score automatically.
