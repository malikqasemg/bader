"""Fast news headlines for Bader (Google News RSS — no browser, ~1 second).

    python news.py "<topic>" [--days 7] [--max 8] [--lang en|ar]

Prints JSON: [{"title", "source", "date"}], newest first (--links adds "link").
Language is detected from the topic when --lang is not given.
"""
import argparse
import email.utils
import json
import re
import sys
import urllib.parse
import urllib.request
import xml.etree.ElementTree as ET


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("topic")
    ap.add_argument("--days", type=int, default=7)
    ap.add_argument("--max", type=int, default=8)
    ap.add_argument("--lang", choices=["en", "ar"])
    ap.add_argument("--links", action="store_true", help="include article links (long)")
    a = ap.parse_args()
    lang = a.lang or ("ar" if re.search(r"[؀-ۿ]", a.topic) else "en")
    loc = "hl=ar&gl=SA&ceid=SA:ar" if lang == "ar" else "hl=en&gl=US&ceid=US:en"
    q = urllib.parse.quote(f"{a.topic} when:{a.days}d")
    url = f"https://news.google.com/rss/search?q={q}&{loc}"
    req = urllib.request.Request(url, headers={"User-Agent": "Mozilla/5.0 (Bader)"})
    try:
        xml = urllib.request.urlopen(req, timeout=15).read()
    except Exception as e:
        sys.exit(f"news fetch failed: {e}")
    items = []
    for it in ET.fromstring(xml).iter("item"):
        title = (it.findtext("title") or "").strip()
        source = (it.findtext("source") or "").strip()
        if source and title.endswith(" - " + source):
            title = title[: -len(source) - 3]
        try:
            when = email.utils.parsedate_to_datetime(it.findtext("pubDate") or "")
        except Exception:
            when = None
        items.append({"title": title, "source": source, "when": when, "link": it.findtext("link") or ""})
    items.sort(key=lambda i: i["when"].timestamp() if i["when"] else 0, reverse=True)
    out = [{"title": i["title"], "source": i["source"],
            "date": i["when"].strftime("%Y-%m-%d") if i["when"] else "", **({"link": i["link"]} if a.links else {})}
           for i in items[: a.max]]
    print(json.dumps(out, ensure_ascii=False, indent=1))


if __name__ == "__main__":
    main()
