// Settings: Accounts — connect Bader to Gmail and Outlook. Webex follows.
// Tokens stay in the engine; this page only shows connected / not connected.

import { Bridge, type AccountsStatus } from "../core/bridge";
import { h, clear } from "../views/dom";

function dot(ok: boolean): HTMLElement {
  return h("i", { class: "dot", style: `background:${ok ? "#22c55e" : "#f4505e"}` });
}

function notice(box: HTMLElement, ok: boolean, text: string) {
  clear(box);
  box.append(h("div", { class: ok ? "notice ok" : "notice err", text }));
}

function field(placeholder: string, value = ""): HTMLInputElement {
  return h("input", {
    type: "text",
    spellcheck: "false",
    autocomplete: "off",
    placeholder,
    value,
    style: "flex:1 1 auto;min-width:0",
  }) as HTMLInputElement;
}

const err = (e: unknown) => String(e).replace(/^Error:\s*/, "");

// ── Gmail ─────────────────────────────────────────────────────────────────────

function gmailBlock(st: AccountsStatus, refresh: () => void): HTMLElement {
  const box = h("div", { class: "account" });
  const feedback = h("div", {});
  const head = h("div", { class: "row" },
    h("label", {}, dot(st.gmail), h("span", { text: " Gmail" })),
    h("span", { class: "hint", text: st.gmail ? "Connected — mail and calendar." : "Not connected." }),
  );
  box.append(head);

  if (st.gmail) {
    const off = h("button", { class: "danger", text: "Disconnect" }) as HTMLButtonElement;
    off.addEventListener("click", async () => {
      try {
        await Bridge.gmailDisconnect();
        refresh();
      } catch (e) {
        notice(feedback, false, err(e));
      }
    });
    box.append(h("div", { class: "row" }, off), feedback);
    return box;
  }

  // Step 1 — Google client file (once per company).
  const path = field("Path to client_secret….json (Google Cloud → Credentials → Desktop app)");
  void Bridge.gmailFindClientFile().then((p) => {
    if (p && !path.value) path.value = p;
  });
  const useFile = h("button", { text: st.gmailClient ? "Replace file" : "Use file" }) as HTMLButtonElement;
  useFile.addEventListener("click", async () => {
    if (!path.value.trim()) return;
    useFile.disabled = true;
    try {
      await Bridge.gmailSetClient(path.value.trim());
      notice(feedback, true, "Google client saved. Now sign in.");
      st.gmailClient = true;
      signIn.disabled = false;
    } catch (e) {
      notice(feedback, false, err(e));
    } finally {
      useFile.disabled = false;
    }
  });

  // Step 2 — sign in, then paste the address the browser lands on.
  const signIn = h("button", { class: "primary", text: "Sign in with Google" }) as HTMLButtonElement;
  signIn.disabled = !st.gmailClient;
  const code = field("After approving, paste the full address from the browser here");
  const finish = h("button", { class: "primary", text: "Finish" }) as HTMLButtonElement;
  const codeRow = h("div", { class: "row" }, code, finish);
  codeRow.style.display = "none";

  signIn.addEventListener("click", async () => {
    signIn.disabled = true;
    try {
      const url = await Bridge.gmailAuthUrl();
      await Bridge.openUrl(url);
      codeRow.style.display = "";
      notice(feedback, true,
        "Google opened in your browser. Approve, then copy the whole address (it may show a page error — that is expected) and paste it below.");
    } catch (e) {
      notice(feedback, false, err(e));
    } finally {
      signIn.disabled = false;
    }
  });
  finish.addEventListener("click", async () => {
    if (!code.value.trim()) return;
    finish.disabled = true;
    try {
      await Bridge.gmailAuthCode(code.value.trim());
      notice(feedback, true, "Gmail connected.");
      refresh();
    } catch (e) {
      notice(feedback, false, err(e));
    } finally {
      finish.disabled = false;
    }
  });

  box.append(
    h("div", { class: "row" }, h("label", { text: "1. Google client" }), path, useFile),
    h("div", { class: "row" }, h("label", { text: "2. Sign in" }), signIn),
    codeRow,
    feedback,
  );
  return box;
}

// ── Outlook ───────────────────────────────────────────────────────────────────

function outlookBlock(st: AccountsStatus, refresh: () => void): HTMLElement {
  const box = h("div", { class: "account" });
  const feedback = h("div", {});
  box.append(h("div", { class: "row" },
    h("label", {}, dot(st.outlook), h("span", { text: " Outlook" })),
    h("span", { class: "hint", text: st.outlook ? "Connected — mail and calendar." : "Not connected." }),
  ));

  if (st.outlook) {
    const off = h("button", { class: "danger", text: "Disconnect" }) as HTMLButtonElement;
    off.addEventListener("click", async () => {
      try {
        await Bridge.outlookDisconnect();
        refresh();
      } catch (e) {
        notice(feedback, false, err(e));
      }
    });
    box.append(h("div", { class: "row" }, off), feedback);
    return box;
  }

  const clientId = field("Application (client) ID from Microsoft Entra", st.outlookClientId);
  const tenant = field("Tenant ID (blank = any account)", st.outlookTenant === "common" ? "" : st.outlookTenant);
  const signIn = h("button", { class: "primary", text: "Sign in with Microsoft" }) as HTMLButtonElement;
  const codeShow = h("div", { class: "device-code" });

  signIn.addEventListener("click", async () => {
    signIn.disabled = true;
    clear(codeShow);
    try {
      const dc = await Bridge.outlookStart(clientId.value.trim(), tenant.value.trim());
      codeShow.append(
        h("div", { class: "hint", text: "Enter this code on the Microsoft page that just opened:" }),
        h("div", { class: "big-code", text: dc.userCode }),
      );
      notice(feedback, true, "Waiting for you to finish signing in…");
      await Bridge.openUrl(dc.verificationUri);
      const who = await Bridge.outlookWait(dc.deviceCode, dc.interval, dc.expiresIn);
      clear(codeShow);
      notice(feedback, true, who ? `Outlook connected as ${who}.` : "Outlook connected.");
      refresh();
    } catch (e) {
      notice(feedback, false, err(e));
    } finally {
      signIn.disabled = false;
    }
  });

  box.append(
    h("div", { class: "row" }, h("label", { text: "Client ID" }), clientId),
    h("div", { class: "row" }, h("label", { text: "Tenant" }), tenant),
    h("div", { class: "row" }, h("label", { text: "" }), signIn),
    codeShow,
    feedback,
  );
  return box;
}

// ── Section ───────────────────────────────────────────────────────────────────

export function accountsSection(): HTMLElement {
  const section = h("section", {});
  const head = dot(false);
  const body = h("div", {});
  section.append(h("h2", {}, head, h("span", { text: "Accounts  ·  الحسابات" })), body);

  async function refresh() {
    const st = await Bridge.accountsStatus();
    clear(body);
    if (!st) {
      body.append(h("span", { class: "hint", text: "Accounts are available in the Bader app." }));
      return;
    }
    head.style.background = st.gmail || st.outlook ? "#22c55e" : "#f4505e";
    body.append(
      h("span", { class: "hint", text: "Connect the mailboxes Bader reads and briefs you on." }),
      outlookBlock(st, () => void refresh()),
      gmailBlock(st, () => void refresh()),
      h("div", { class: "account" },
        h("div", { class: "row" },
          h("label", {}, dot(false), h("span", { text: " Webex" })),
          h("span", { class: "hint", text: "Coming next." }),
        ),
      ),
    );
  }
  body.append(h("span", { class: "hint", text: "Checking accounts…" }));
  void refresh();
  return section;
}
