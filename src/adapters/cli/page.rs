//! `qm due --html`: the lists of `qm due` as one self-contained web page (ADR 0012). The page
//! is a file the user opens; nothing serves it. Every text that came from a receipt or from
//! the user is escaped, and the page loads nothing from the network.

use crate::application::recommend::{HabitReport, HabitRow};

use super::label;

/// A text as HTML character data or a quoted attribute value.
fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            other => out.push(other),
        }
    }
    out
}

const STYLE: &str = r#"
:root { --bg:#faf9f6; --card:#fff; --ink:#1f2328; --soft:#667085; --line:#e4e2dc; --accent:#2f6f4f; --tint:#e8f2ec; }
@media (prefers-color-scheme: dark) {
  :root { --bg:#16181b; --card:#1f2226; --ink:#e8e6e1; --soft:#9aa3ad; --line:#30343a; --accent:#7fc7a0; --tint:#233029; }
}
* { box-sizing: border-box; }
body { margin:0; background:var(--bg); color:var(--ink); font:16px/1.45 system-ui, -apple-system, "Segoe UI", sans-serif; }
main { max-width:860px; margin:0 auto; padding:24px 16px 96px; }
h1 { font-size:1.6rem; margin:0 0 4px; }
.sub { color:var(--soft); margin:0 0 24px; }
section { background:var(--card); border:1px solid var(--line); border-radius:12px; padding:16px; margin:0 0 16px; }
h2 { font-size:1.05rem; margin:0; }
.why { color:var(--soft); font-size:.9rem; margin:2px 0 10px; }
ul { list-style:none; margin:0; padding:0; }
li { border-top:1px solid var(--line); }
li:first-child { border-top:0; }
label { display:flex; gap:12px; align-items:flex-start; padding:10px 4px; cursor:pointer; }
input[type=checkbox] { width:20px; height:20px; margin-top:2px; accent-color:var(--accent); flex:none; }
.name { font-weight:600; overflow-wrap:anywhere; }
.meta { color:var(--soft); font-size:.88rem; }
li:has(input:checked) { background:var(--tint); }
.tag { display:inline-block; border:1px solid var(--line); border-radius:999px; padding:0 8px; font-size:.8rem; color:var(--soft); margin-left:6px; }
.plain li { padding:10px 4px; }
.none { color:var(--soft); margin:0; }
footer { position:fixed; left:0; right:0; bottom:0; background:var(--card); border-top:1px solid var(--line); padding:10px 16px; display:flex; gap:12px; align-items:center; justify-content:center; }
button { font:inherit; border:0; border-radius:8px; padding:8px 14px; background:var(--accent); color:var(--bg); cursor:pointer; }
button.quiet { background:transparent; color:var(--soft); border:1px solid var(--line); }
#count { color:var(--soft); }
"#;

/// Ticks live in the page only; the script counts them and copies the ticked names.
const SCRIPT: &str = r#"
const boxes = () => [...document.querySelectorAll('input[type=checkbox]')];
const count = document.getElementById('count');
const refresh = () => { count.textContent = boxes().filter(b => b.checked).length + ' ticked'; };
document.addEventListener('change', refresh);
document.getElementById('essentials-all').addEventListener('click', () => {
  document.querySelectorAll('#essentials input').forEach(b => { b.checked = true; }); refresh();
});
document.getElementById('clear').addEventListener('click', () => { boxes().forEach(b => { b.checked = false; }); refresh(); });
document.getElementById('copy').addEventListener('click', async (e) => {
  const text = boxes().filter(b => b.checked).map(b => b.dataset.name).join('\n');
  try { await navigator.clipboard.writeText(text); e.target.textContent = 'Copied'; }
  catch { e.target.textContent = 'Copy failed'; }
  setTimeout(() => { e.target.textContent = 'Copy ticked'; }, 1500);
});
refresh();
"#;

fn tick_row(out: &mut String, name: &str, meta: &str, ticked: bool) {
    out.push_str(&format!(
        "<li><label><input type=\"checkbox\" data-name=\"{name}\"{checked}>\
         <span><span class=\"name\">{name}</span><br><span class=\"meta\">{meta}</span></span></label></li>\n",
        name = escape(name),
        meta = escape(meta),
        checked = if ticked { " checked" } else { "" },
    ));
}

fn habit_rows(out: &mut String, rows: &[HabitRow], ticked: bool) {
    if rows.is_empty() {
        out.push_str("<p class=\"none\">none</p>\n");
        return;
    }
    out.push_str("<ul>\n");
    for row in rows {
        let r = &row.recommendation;
        let meta = format!(
            "in {} of your latest {} orders \u{b7} last bought {} \u{b7} usually {} \u{b7} last paid {}",
            r.in_recent, r.recent_orders, r.last_bought, r.typical_quantity, r.last_price
        );
        tick_row(out, &label(&row.product), &meta, ticked);
    }
    out.push_str("</ul>\n");
}

/// The whole page. The essentials start ticked: they are the recommendation; everything else
/// is a choice.
pub fn render_habit_page(report: &HabitReport) -> String {
    let mut out = String::from(
        "<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
         <meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; \
         style-src 'unsafe-inline'; script-src 'unsafe-inline'; base-uri 'none'; form-action 'none'\">\n\
         <meta name=\"referrer\" content=\"no-referrer\">\n<title>Next order</title>\n<style>",
    );
    out.push_str(STYLE);
    out.push_str("</style>\n</head>\n<body>\n<main>\n<h1>Next order</h1>\n");
    out.push_str(&format!(
        "<p class=\"sub\">As of {}, from {} orders. Run <code>qm due --html</code> again for a new page.</p>\n",
        report.as_of, report.orders
    ));

    out.push_str(
        "<section id=\"essentials\">\n<h2>Essentials</h2>\n\
         <p class=\"why\">The recommendation: in at least a quarter of your latest orders.</p>\n",
    );
    habit_rows(&mut out, &report.essentials, true);
    out.push_str("</section>\n");

    out.push_str("<section>\n<h2>Your additions</h2>\n<p class=\"why\">Added with <code>qm add</code>, with the reason.</p>\n");
    if report.additions.is_empty() {
        out.push_str("<p class=\"none\">none</p>\n");
    } else {
        out.push_str("<ul>\n");
        for addition in &report.additions {
            let meta = format!(
                "{} \u{b7} added {} \u{b7} number {}",
                addition.reason.phrase(),
                addition.added_on,
                addition.id
            );
            tick_row(&mut out, addition.item.as_str(), &meta, true);
        }
        out.push_str("</ul>\n");
    }
    out.push_str("</section>\n");

    out.push_str(
        "<section>\n<h2>Extras</h2>\n\
         <p class=\"why\">Other choices: the next most often in your latest orders.</p>\n",
    );
    habit_rows(&mut out, &report.extras, false);
    out.push_str("</section>\n");

    out.push_str(
        "<section>\n<h2>Remember this?</h2>\n\
         <p class=\"why\">You used to buy these; they are in none of your latest orders. A new draw each day.</p>\n",
    );
    if report.remember.is_empty() {
        out.push_str("<p class=\"none\">none</p>\n");
    } else {
        out.push_str("<ul>\n");
        for row in &report.remember {
            let meta = format!(
                "in {} orders \u{b7} last bought {} \u{b7} last paid {}",
                row.reminder.orders_with_product, row.reminder.last_bought, row.reminder.last_price
            );
            tick_row(&mut out, &label(&row.product), &meta, false);
        }
        out.push_str("</ul>\n");
    }
    out.push_str("</section>\n");

    if !report.stocked.is_empty() {
        out.push_str(
            "<section class=\"plain\">\n<h2>Stocked</h2>\n\
             <p class=\"why\">Left out: the last purchase was a bulk buy.</p>\n<ul>\n",
        );
        for row in &report.stocked {
            if let Some(until) = row.recommendation.stocked_until {
                out.push_str(&format!(
                    "<li><span class=\"name\">{}</span><br><span class=\"meta\">bought {} \u{b7} expected to last until {}</span></li>\n",
                    escape(&label(&row.product)),
                    row.recommendation.last_bought,
                    until
                ));
            }
        }
        out.push_str("</ul>\n</section>\n");
    }

    out.push_str(
        "</main>\n<footer><span id=\"count\"></span>\
         <button id=\"copy\">Copy ticked</button>\
         <button class=\"quiet\" id=\"essentials-all\">Tick essentials</button>\
         <button class=\"quiet\" id=\"clear\">Clear</button></footer>\n<script>",
    );
    out.push_str(SCRIPT);
    out.push_str("</script>\n</body>\n</html>\n");
    out
}

#[cfg(test)]
mod tests {
    use jiff::civil::Date;

    use super::*;
    use crate::application::ports::ProductLabel;
    use crate::application::recommend::ReminderRow;
    use crate::domain::ids::{AdditionId, ProductId};
    use crate::domain::list::{Addition, ItemText};
    use crate::domain::money::{Money, Quantity};
    use crate::domain::recommend::{HabitRecommendation, Reminder};
    use crate::domain::states::{AddReason, SoldBy};

    fn day(d: i8) -> Date {
        jiff::civil::date(2026, 3, d)
    }

    fn product(id: i64, name: &str) -> ProductLabel {
        ProductLabel {
            id: ProductId::new(id).expect("id"),
            display_name: name.to_owned(),
            size_text: Some("1/2 gallon".to_owned()),
            sold_by: SoldBy::Each,
            department: None,
            latest_raw_name: name.to_owned(),
        }
    }

    fn habit_row(id: i64, name: &str, stocked_until: Option<Date>) -> HabitRow {
        HabitRow {
            product: product(id, name),
            recommendation: HabitRecommendation {
                product: ProductId::new(id).expect("id"),
                in_recent: 15,
                recent_orders: 20,
                in_long: 35,
                long_orders: 50,
                last_bought: day(3),
                typical_quantity: Quantity::parse("2").expect("qty"),
                last_price: Money::from_cents(579),
                stocked_until,
            },
        }
    }

    #[test]
    fn the_page_holds_every_list_escapes_its_texts_and_loads_nothing() {
        let report = HabitReport {
            as_of: day(7),
            orders: 42,
            essentials: vec![habit_row(1, "Whole Milk <b>\"A&B\"</b>", None)],
            extras: vec![habit_row(2, "Ketchup", None)],
            remember: vec![ReminderRow {
                product: product(3, "Bagels"),
                reminder: Reminder {
                    product: ProductId::new(3).expect("id"),
                    orders_with_product: 16,
                    last_bought: day(1),
                    last_price: Money::from_cents(399),
                },
            }],
            seed: 1,
            additions: vec![Addition {
                id: AdditionId::new(9).expect("id"),
                item: ItemText::parse("miso <script>alert(1)</script>").expect("text"),
                product: None,
                reason: AddReason::NewFood,
                added_on: day(5),
            }],
            stocked: vec![habit_row(4, "Seaweed Snack", Some(day(30)))],
        };
        let page = render_habit_page(&report);

        // A receipt name and a typed item reach the page as text, never as markup.
        assert!(page.contains("Whole Milk &lt;b&gt;&quot;A&amp;B&quot;&lt;/b&gt; (1/2 gallon)"));
        assert!(page.contains("miso &lt;script&gt;alert(1)&lt;/script&gt;"));
        assert!(!page.contains("<script>alert"));
        assert_eq!(page.matches("<script>").count(), 1);
        // Nothing is loaded from anywhere.
        assert!(page.contains("default-src 'none'"));
        assert!(page.contains("base-uri 'none'; form-action 'none'"));
        assert!(!page.contains("http://") && !page.contains("https://"));
        assert!(!page.contains(" src=") && !page.contains(" href="));

        // Essentials and additions start ticked; extras and reminders do not.
        let ticked = |name: &str| {
            let line = page
                .lines()
                .find(|l| l.contains(&format!("data-name=\"{name}")))
                .expect("listed");
            line.contains(" checked>")
        };
        assert!(ticked("Whole Milk") && ticked("miso"));
        assert!(!ticked("Ketchup") && !ticked("Bagels"));
        assert!(page.contains("in 15 of your latest 20 orders"));
        assert!(page.contains("new food to try \u{b7} added 2026-03-05 \u{b7} number 9"));
        assert!(page.contains("in 16 orders \u{b7} last bought 2026-03-01 \u{b7} last paid $3.99"));
        assert!(page.contains("Seaweed Snack (1/2 gallon)</span><br><span class=\"meta\">bought 2026-03-03 \u{b7} expected to last until 2026-03-30"));
        assert!(page.starts_with("<!doctype html>") && page.ends_with("</html>\n"));

        // An empty history gives a page with "none", not an error.
        let empty = HabitReport {
            essentials: vec![],
            extras: vec![],
            remember: vec![],
            additions: vec![],
            stocked: vec![],
            ..report
        };
        let page = render_habit_page(&empty);
        assert_eq!(page.matches("<p class=\"none\">none</p>").count(), 4);
        assert!(!page.contains("<h2>Stocked</h2>"));
    }
}
