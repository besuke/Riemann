"""Fetch the month-end BB JGB closing prices of Japan Bond Trading (JBTS) and save them normalized.

    python -X utf8 tools/market/fetch_jbts_bb_close.py 2026-09 2026-10

Source: https://www.bb.jbts.co.jp/ja/historical/bb_hikene.html (the page shows the last three months; a month appears
on the fifth business day of the next month). Writes, under market_data/bb_price/ (ignored by git: market data
is not committed or shown, see CLAUDE.md):

    raw/bb_hikene_<retrieved date>.html     the page as fetched
    bb_price_<yyyymmdd>.txt                 one table per month, '|'-separated with a header; yyyymmdd is the month's
                                            reference date (the site's 基準日 column, most rows)
    PROVENANCE.md                           what was fetched and when

Each month's tables are classified by their issue names, not by their position (a month may lack a table). Values
are kept as published (nothing rounded or recomputed).
"""

from __future__ import annotations

import csv
import datetime as dt
import html
import re
import sys
import urllib.request
from pathlib import Path

URL = "https://www.bb.jbts.co.jp/ja/historical/bb_hikene.html"
ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / "market_data" / "bb_price"
COLUMNS = [
    "month", "reference_date", "category", "issue_label", "bond_type", "series", "when_issued",
    "coupon_pct", "remaining_years", "maturity_date", "simple_yield_pct", "compound_yield_pct", "price",
]
TYPES = {"40Y": "JGB40", "30Y": "JGB30", "20Y": "JGB20", "10Y": "JGB10", "5Y": "JGB5", "2Y": "JGB2",
         "TDB": "TDB", "TIPS": "JGBi10"}


def classify(label: str) -> tuple[str, str, bool]:
    """(category, series, when_issued) from a published issue label such as 40/19, 384, WI10-11, T1414/Y, TIPS10/31."""
    m = re.fullmatch(r"WI(\d+)-(\d+)", label)
    if m:
        return f"{m.group(1)}Y", f"WI-{m.group(2)}", True
    m = re.fullmatch(r"TIPS10/(\d+)", label)
    if m:
        return "TIPS", m.group(1), False
    m = re.fullmatch(r"T(\d+/\w+)", label)
    if m:
        return "TDB", m.group(1), False
    m = re.fullmatch(r"(40|30|20|5|2)/(\d+)", label)
    if m:
        return f"{m.group(1)}Y", m.group(2), False
    if re.fullmatch(r"\d+", label):
        return "10Y", label, False
    raise ValueError(f"unknown issue label: {label!r}")


def num(x: str) -> str:
    """A published number as a plain decimal string, or empty when not published."""
    x = x.strip()
    if x in ("", "-"):
        return ""
    float(x)
    return x


def iso(x: str) -> str:
    return "" if x.strip() in ("", "-") else dt.datetime.strptime(x.strip(), "%Y/%m/%d").date().isoformat()


def parse(page: str) -> dict[str, list[dict[str, str]]]:
    labels = re.findall(r'<a href="#hikene0\d">(\d{4}/\d{2})</a>', page)
    months = [l.replace("/", "-") for l in dict.fromkeys(labels)]
    tables = []
    for m in re.finditer(r'<table class="tbCore">(.*?)</table>', page, re.S):
        rows = [[html.unescape(c).strip() for c in re.findall(r"<td[^>]*>([^<]*)</td>", r)]
                for r in re.findall(r"<tr>(.*?)</tr>", m.group(1), re.S)]
        rows = [r for r in rows if len(r) == 8 and r[0] not in ("銘柄名（回号）", "-")]
        if rows:
            tables.append(rows)
    # The tables come month by month, newest first, each month starting with the 40-year table.
    groups: list[list[list[str]]] = []
    for t in tables:
        if classify(t[0][0])[0] == "40Y" or not groups:
            groups.append([])
        groups[-1].extend(t)
    if len(groups) != len(months):
        raise ValueError(f"{len(groups)} groups of tables for {len(months)} months")
    out = {}
    for month, rows in zip(months, groups):
        recs = []
        for r in rows:
            cat, series, wi = classify(r[0])
            recs.append({
                "month": month, "reference_date": iso(r[7]), "category": cat, "issue_label": r[0], "bond_type": TYPES[cat],
                "series": series, "when_issued": "TRUE" if wi else "FALSE", "coupon_pct": num(r[1]), "remaining_years": num(r[2]),
                "maturity_date": iso(r[3]), "simple_yield_pct": num(r[4]), "compound_yield_pct": num(r[5]), "price": num(r[6]),
            })
        out[month] = recs
    return out


def main(wanted: list[str]) -> None:
    req = urllib.request.Request(URL, headers={"User-Agent": "Mozilla/5.0"})
    page = urllib.request.urlopen(req, timeout=60).read().decode("utf-8")
    retrieved = dt.datetime.now().astimezone()
    (OUT / "raw").mkdir(parents=True, exist_ok=True)
    raw = OUT / "raw" / f"bb_hikene_{retrieved:%Y-%m-%d}.html"
    raw.write_text(page, encoding="utf-8")
    data = parse(page)
    log = []
    for month in wanted:
        if month not in data:
            log.append(f"| {month} | not published yet (the page has {', '.join(data)}) | | | |")
            # the file name needs the month's reference date, which is only known once the month is published
            print(f"{month}: not on the page (available: {', '.join(data)})")
            continue
        recs = data[month]
        main_ref = max({r["reference_date"] for r in recs}, key=lambda d: sum(r["reference_date"] == d for r in recs))
        path = OUT / f"bb_price_{main_ref.replace('-', '')}.txt"
        with path.open("w", newline="", encoding="utf-8") as f:
            w = csv.DictWriter(f, fieldnames=COLUMNS, delimiter="|", lineterminator="\n")
            w.writeheader()
            w.writerows(recs)
        refs = sorted({r["reference_date"] for r in recs})
        cats = {c: sum(r["category"] == c for r in recs) for c in TYPES}
        log.append(f"| {month} | {path.name} | {len(recs)} | {', '.join(refs)} | {', '.join(f'{k} {v}' for k, v in cats.items() if v)} |")
        print(f"{month}: {len(recs)} rows -> {path}  reference dates {refs}")
    prov = OUT / "PROVENANCE.md"
    head = "" if prov.exists() else (
        "# JBTS BB JGB closing prices\n\n"
        f"Source: {URL} (Japan Bond Trading Co., month-end BB closing prices). Fetched by `tools/market/fetch_jbts_bb_close.py`.\n"
        "Market data: kept out of git (`.gitignore`) and not shown in tables or output (CLAUDE.md). Check the site's terms before any redistribution.\n\n"
        "Files: `bb_price_<yyyymmdd>.txt`, UTF-8, `|`-separated with a header. Columns: " + ", ".join(f"`{c}`" for c in COLUMNS) + ". Values as published; empty when not published "
        "(when-issued rows have no coupon or price). `reference_date` is the site's 基準日 column, as published.\n\n"
        "| month | file | rows | reference dates | rows per category |\n|---|---|---|---|---|\n")
    with prov.open("a", encoding="utf-8") as f:
        f.write(head)
        f.write(f"| fetched {retrieved:%Y-%m-%d %H:%M %z} | raw/{raw.name} | | | |\n")
        f.write("\n".join(log) + "\n")


if __name__ == "__main__":
    main(sys.argv[1:] or ["2026-09", "2026-10", "2026-11"])
