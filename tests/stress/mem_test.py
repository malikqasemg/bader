import stress
words = stress.MAIL_WORDS
stress.MAIL_WORDS = []          # like Telegram: no snapshot injected, engine must recall by itself
r = stress.ask("What was the last meeting transcript you summarised for me? Give me its action items.", force_engine=True)
print("TELEGRAM-LIKE", r.get("total"), r.get("tools"), r.get("error"), "\n", str(r.get("answer"))[:600], "\n---", flush=True)
stress.MAIL_WORDS = words
for q in ["What did I ask you yesterday about the weather, and what was the answer?", "ماذا طلبت منك أمس بخصوص أخبار سيسكو؟"]:
    r = stress.ask(q)
    print(r.get("lane"), r.get("total"), r.get("tools"), r.get("error"), "\n", str(r.get("answer"))[:500], "\n---", flush=True)
