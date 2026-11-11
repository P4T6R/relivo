import React, {
  useEffect,
  useRef,
  useState,
  type FormEvent,
  type ReactNode,
} from "react";
import { createRoot } from "react-dom/client";
import {
  Activity,
  ArrowDownLeft,
  ArrowUpRight,
  Check,
  ChevronLeft,
  ChevronRight,
  ChevronDown,
  Copy,
  Folder,
  LayoutDashboard,
  LogOut,
  Monitor,
  MoreHorizontal,
  Plus,
  RefreshCw,
  Search,
  Settings2,
  Shield,
  ShieldCheck,
  Terminal,
  Users,
  X,
  KeyRound,
  LoaderCircle,
  ArrowRight,
  Globe,
  LockKeyhole,
} from "lucide-react";
import {
  all,
  request,
  setCsrf,
  type User,
  type Device,
  type Group,
  type Grant,
  type Audit,
  type Overview,
  type Listing,
} from "./api";
import "./style.css";

type Page = "overview" | "devices" | "users" | "groups" | "grants" | "audit";
type Editor =
  | { kind: "user"; item?: User }
  | { kind: "device"; item?: Device }
  | { kind: "group"; item?: Group }
  | { kind: "grant" }
  | { kind: "credential"; credential: string; id: string }
  | {
      kind: "confirm";
      title: string;
      message: string;
      run: () => Promise<unknown>;
    };
const nav = [
  { id: "overview", name: "Overview", icon: LayoutDashboard },
  { id: "devices", name: "Devices", icon: Monitor },
  { id: "users", name: "People", icon: Users },
  { id: "groups", name: "Groups", icon: Folder },
  { id: "grants", name: "Access rules", icon: ShieldCheck },
  { id: "audit", name: "Activity", icon: Activity },
] as const;
const titles: Record<Page, [string, string]> = {
  overview: [
    "Your control room",
    "A clear view of your devices, people and access.",
  ],
  devices: ["Devices", "Enroll, organize and manage your remote devices."],
  users: ["People", "The people who can access your workspace."],
  groups: ["Groups", "Bring your people and devices into the right groups."],
  grants: [
    "Access rules",
    "Decide who can see and connect to each device group.",
  ],
  audit: ["Activity", "A record of access decisions and workspace changes."],
};
const time = (value: number | null) =>
  value
    ? new Date(value * 1000).toLocaleString(undefined, {
        month: "short",
        day: "numeric",
        hour: "2-digit",
        minute: "2-digit",
      })
    : "No report yet";
const errorText = (e: unknown) =>
  e instanceof Error ? e.message : "Something went wrong.";
function Badge({
  children,
  tone = "neutral",
}: {
  children: ReactNode;
  tone?: string;
}) {
  return (
    <span className={"badge " + tone}>
      <span />
      {children}
    </span>
  );
}
function App() {
  const [user, setUser] = useState<User | null>(null),
    [initial, setInitial] = useState(true),
    [page, setPage] = useState<Page>("overview");
  const [search, setSearch] = useState(""),
    [query, setQuery] = useState(""),
    [number, setNumber] = useState(1),
    [refresh, setRefresh] = useState(0);
  const [rows, setRows] = useState<(Device | User | Group | Grant | Audit)[]>(
      [],
    ),
    [total, setTotal] = useState(0),
    [stats, setStats] = useState<Overview | null>(null);
  const [busy, setBusy] = useState(false),
    [error, setError] = useState(""),
    [notice, setNotice] = useState(""),
    [editor, setEditor] = useState<Editor | null>(null);
  const [groupOptions, setGroupOptions] = useState<Group[]>([]),
    [peopleOptions, setPeopleOptions] = useState<User[]>([]),
    [deviceOptions, setDeviceOptions] = useState<Device[]>([]);
  const admin = user?.role === "admin";
  useEffect(() => {
    request<{ user: User; csrf: string }>("/session")
      .then((s) => {
        setUser(s.user);
        setCsrf(s.csrf);
      })
      .catch(() => {})
      .finally(() => setInitial(false));
    const expired = () => {
      setUser(null);
      setCsrf("");
      setRows([]);
      setEditor(null);
    };
    window.addEventListener("session-expired", expired);
    return () => window.removeEventListener("session-expired", expired);
  }, []);
  useEffect(() => {
    const timer = setTimeout(() => {
      setQuery(search);
      setNumber(1);
    }, 250);
    return () => clearTimeout(timer);
  }, [search]);
  useEffect(() => {
    if (!user) return;
    let cancelled = false;
    setBusy(true);
    setError("");
    setRows([]);
    Promise.all([
      request<Overview>("/overview"),
      request<Listing<Device | User | Group | Grant | Audit>>(
        `/${page === "overview" ? "devices" : page}?page=${number}&limit=${page === "overview" ? 5 : 25}&q=${encodeURIComponent(query)}`,
      ),
      admin ? all<Group>("groups") : Promise.resolve([]),
    ])
      .then(([overview, list, groups]) => {
        if (!cancelled) {
          setStats(overview);
          setRows(list.items);
          setTotal(list.total);
          setGroupOptions(groups);
        }
      })
      .catch((e) => {
        if (!cancelled) setError(errorText(e));
      })
      .finally(() => {
        if (!cancelled) setBusy(false);
      });
    return () => {
      cancelled = true;
    };
  }, [user, page, query, number, refresh, admin]);
  useEffect(() => {
    if (!notice) return;
    const timer = setTimeout(() => setNotice(""), 5000);
    return () => clearTimeout(timer);
  }, [notice]);
  // Clear the previous table in the same render as navigation: its row shape
  // belongs to the old page, and effects only run after the new page renders.
  function navigate(next: Page) {
    setRows([]);
    setTotal(0);
    setBusy(true);
    setPage(next);
    setSearch("");
    setQuery("");
    setNumber(1);
    setError("");
    setRefresh((v) => v + 1);
  }
  async function edit(next: Editor) {
    setError("");
    try {
      if (next.kind === "group") {
        const [people, devices] = await Promise.all([
          all<User>("users"),
          all<Device>("devices"),
        ]);
        setPeopleOptions(people);
        setDeviceOptions(devices);
      }
      setEditor(next);
    } catch (e) {
      setError(errorText(e));
    }
  }
  function done(message = "Changes saved") {
    setEditor(null);
    setRefresh((v) => v + 1);
    setNotice(message);
  }
  async function signout() {
    try {
      await request("/session", "DELETE");
      setUser(null);
      setCsrf("");
      setEditor(null);
    } catch (e) {
      setError(errorText(e));
    }
  }
  function connect(d: Device) {
    window.location.href = `rustdesk://${encodeURIComponent(d.id)}`;
    setNotice(
      "Opening Relivo Agent. Sign in there with your workspace account.",
    );
  }
  const groupName = (id: string) =>
    groupOptions.find((g) => g.id === id)?.name || id.slice(0, 12);
  if (initial)
    return (
      <div className="loading-screen">
        <LoaderCircle className="spin" />
        <span>Opening your workspace…</span>
      </div>
    );
  if (!user)
    return (
      <Login
        onLogin={(u, c) => {
          setUser(u);
          setCsrf(c);
          navigate("overview");
        }}
      />
    );
  return (
    <div className="shell">
      <aside className="sidebar">
        <a
          href="#"
          className="brand"
          aria-label="Relivo Console overview"
          onClick={(e) => {
            e.preventDefault();
            navigate("overview");
          }}
        >
          <span className="brand-icon">
            <ArrowDownLeft size={18} />
            <ArrowUpRight size={18} />
          </span>
          <span>
            Relivo<span className="brand-sub">CONSOLE</span>
          </span>
        </a>
        <div className="workspace">
          <span className="workspace-mark">W</span>
          <div>
            My workspace<small>Self-hosted</small>
          </div>
          <LockKeyhole size={14} />
        </div>
        <p className="nav-label">WORKSPACE</p>
        <nav aria-label="Main navigation">
          {nav
            .filter(
              (n) => admin || !["users", "grants", "audit"].includes(n.id),
            )
            .map((n) => (
              <button
                key={n.id}
                className={page === n.id ? "selected" : ""}
                onClick={() => navigate(n.id)}
                aria-current={page === n.id ? "page" : undefined}
              >
                <n.icon size={18} />
                {n.name}
                {n.id === "devices" && stats ? (
                  <span className="nav-count">{stats.devices}</span>
                ) : null}
              </button>
            ))}
        </nav>
        <div className="sidebar-bottom">
          <div className="private-note">
            <Shield size={18} />
            <div>
              Your infrastructure.
              <br />
              <span>Your control.</span>
            </div>
          </div>
          <button className="account" onClick={signout} title="Sign out">
            <span className="avatar">
              {user.display_name.slice(0, 1).toUpperCase()}
            </span>
            <span>
              {user.display_name}
              <small>{user.role}</small>
            </span>
            <LogOut size={16} />
          </button>
        </div>
      </aside>
      <div className="main">
        <header className="topbar">
          <div className="breadcrumb">
            Workspace <span>/</span>{" "}
            <strong>{nav.find((n) => n.id === page)?.name}</strong>
          </div>
          <div className="topbar-right">
            <span className="private-pill">
              <LockKeyhole size={12} />
              Private workspace
            </span>
            <span className="avatar small">
              {user.display_name.slice(0, 1).toUpperCase()}
            </span>
          </div>
        </header>
        <main>
          <div className="page-heading">
            <div>
              <div className="eyebrow">
                WORKSPACE /{" "}
                {page === "overview"
                  ? "AT A GLANCE"
                  : nav.find((n) => n.id === page)?.name.toUpperCase()}
              </div>
              <h1>{titles[page][0]}</h1>
              <p>{titles[page][1]}</p>
            </div>
            <div className="heading-actions">
              <button
                className="button secondary icon-button"
                onClick={() => setRefresh((v) => v + 1)}
                title="Refresh"
                aria-label="Refresh"
              >
                <RefreshCw size={17} className={busy ? "spin" : ""} />
              </button>
              {admin && page !== "audit" && (
                <button
                  className="button primary"
                  onClick={() =>
                    edit(
                      page === "users"
                        ? { kind: "user" }
                        : page === "groups"
                          ? { kind: "group" }
                          : page === "grants"
                            ? { kind: "grant" }
                            : { kind: "device" },
                    )
                  }
                >
                  <Plus size={17} />
                  {page === "users"
                    ? "Add person"
                    : page === "groups"
                      ? "Create group"
                      : page === "grants"
                        ? "Add rule"
                        : "Enroll device"}
                </button>
              )}
            </div>
          </div>
          {error && (
            <div className="alert" role="alert">
              {error}
              <button onClick={() => setRefresh((v) => v + 1)}>
                Try again
              </button>
            </div>
          )}
          {page === "overview" && (
            <>
              <div className="stats-grid">
                <Stat
                  label="Total devices"
                  value={stats?.devices}
                  icon={<Monitor />}
                  description="In your accessible fleet"
                />
                <Stat
                  label="Recently seen"
                  value={stats?.recently_seen}
                  icon={<Activity />}
                  description="Reported in the last 2 minutes"
                  accent
                />
                <Stat
                  label="Awaiting approval"
                  value={stats?.pending}
                  icon={<Shield />}
                  description="Enrollment needs a review"
                />
                <Stat
                  label={admin ? "Enabled people" : "Your access"}
                  value={admin ? stats?.enabled_users : user.role}
                  icon={<Users />}
                  description={
                    admin
                      ? "Accounts in this workspace"
                      : "Permissions applied by the server"
                  }
                />
              </div>
              <div className="overview-note">
                <span className="note-icon">
                  <ShieldCheck size={21} />
                </span>
                <div>
                  <strong>Access starts with an explicit permission.</strong>
                  <p>
                    Enrolled devices need approval. Operators connect only to
                    device groups they have been granted access to.
                  </p>
                </div>
                <button onClick={() => navigate(admin ? "grants" : "groups")}>
                  Review access <ArrowRight size={16} />
                </button>
              </div>
            </>
          )}
          <section className="panel">
            <div className="panel-header">
              <div>
                <h2>
                  {page === "overview"
                    ? "Your devices"
                    : page === "grants"
                      ? "Permission assignments"
                      : page === "audit"
                        ? "Activity journal"
                        : nav.find((n) => n.id === page)?.name}
                  <span className="count">{total}</span>
                </h2>
                <p>
                  {page === "overview"
                    ? "Your fleet, without the noise."
                    : page === "audit"
                      ? "Server observations and client reports are identified separately."
                      : page === "grants"
                        ? "A connect rule includes visibility. Viewer accounts remain read-only."
                        : "Changes take effect for new requests and connections."}
                </p>
              </div>
              {page === "overview" ? (
                <button
                  className="text-button"
                  onClick={() => navigate("devices")}
                >
                  View all devices <ArrowUpRight size={15} />
                </button>
              ) : (
                page !== "grants" && (
                  <label className="search">
                    <Search size={16} />
                    <input
                      aria-label="Search records"
                      placeholder={
                        page === "audit"
                          ? "Search actor, action, outcome…"
                          : "Search " +
                            nav.find((n) => n.id === page)?.name.toLowerCase() +
                            "…"
                      }
                      value={search}
                      onChange={(e) => setSearch(e.target.value)}
                    />
                    <kbd>/</kbd>
                  </label>
                )
              )}
            </div>
            {busy ? (
              <div className="table-loading">
                <LoaderCircle className="spin" size={23} />
                <span>Loading records…</span>
              </div>
            ) : rows.length === 0 ? (
              <div className="empty">
                <span>
                  <Monitor size={26} />
                </span>
                <h3>{query ? "No matching records" : "A fresh start."}</h3>
                <p>
                  {query
                    ? "Try a different search."
                    : page === "overview" || page === "devices"
                      ? "Enroll your first device to start building your fleet."
                      : page === "audit"
                        ? "Workspace activity will appear here."
                        : "Add your first record using the button above."}
                </p>
                {admin &&
                  !query &&
                  (page === "overview" || page === "devices") && (
                    <button
                      className="button secondary"
                      onClick={() => edit({ kind: "device" })}
                    >
                      <Plus size={16} />
                      Enroll a device
                    </button>
                  )}
              </div>
            ) : (
              <div className="table-scroll">
                <table>
                  {(page === "devices" || page === "overview") && (
                    <>
                      <thead>
                        <tr>
                          <th>DEVICE</th>
                          <th>STATE</th>
                          <th>PLATFORM</th>
                          <th>LAST REPORT</th>
                          <th className="align-right">ACTIONS</th>
                        </tr>
                      </thead>
                      <tbody>
                        {(rows as Device[]).map((d) => (
                          <tr key={d.id}>
                            <td>
                              <div className="device-cell">
                                <span className="device-icon">
                                  <Monitor size={20} />
                                </span>
                                <div>
                                  <strong>{d.name}</strong>
                                  <code>{d.id}</code>
                                </div>
                              </div>
                            </td>
                            <td>
                              <Badge
                                tone={
                                  d.state === "approved"
                                    ? "green"
                                    : d.state === "pending"
                                      ? "amber"
                                      : "neutral"
                                }
                              >
                                {d.state === "pending"
                                  ? d.verified
                                    ? "Review enrollment"
                                    : "Awaiting device"
                                  : d.state}
                              </Badge>
                            </td>
                            <td>
                              <span className="muted">
                                {d.platform || "Not reported"}
                              </span>
                            </td>
                            <td className="muted">{time(d.last_seen)}</td>
                            <td>
                              <div className="row-actions">
                                {d.can_connect && (
                                  <button
                                    className="connect"
                                    onClick={() => connect(d)}
                                  >
                                    Connect <ArrowUpRight size={14} />
                                  </button>
                                )}
                                {admin && (
                                  <button
                                    className="row-edit"
                                    aria-label={"Edit " + d.name}
                                    onClick={() =>
                                      edit({ kind: "device", item: d })
                                    }
                                  >
                                    <Settings2 size={16} />
                                  </button>
                                )}
                              </div>
                            </td>
                          </tr>
                        ))}
                      </tbody>
                    </>
                  )}
                  {page === "users" && (
                    <>
                      <thead>
                        <tr>
                          <th>PERSON</th>
                          <th>ROLE</th>
                          <th>STATUS</th>
                          <th>ADDED</th>
                          <th />
                        </tr>
                      </thead>
                      <tbody>
                        {(rows as User[]).map((u) => (
                          <tr key={u.id}>
                            <td>
                              <div className="device-cell">
                                <span className="avatar pale">
                                  {u.display_name.slice(0, 1)}
                                </span>
                                <div>
                                  <strong>
                                    {u.display_name}
                                    {u.id === user.id && (
                                      <small className="you">You</small>
                                    )}
                                  </strong>
                                  <code>{u.username}</code>
                                </div>
                              </div>
                            </td>
                            <td className="capitalize">{u.role}</td>
                            <td>
                              <Badge tone={u.enabled ? "green" : "neutral"}>
                                {u.enabled ? "Enabled" : "Disabled"}
                              </Badge>
                            </td>
                            <td className="muted">{time(u.created_at)}</td>
                            <td>
                              <button
                                className="row-edit"
                                aria-label={"Edit " + u.username}
                                onClick={() => edit({ kind: "user", item: u })}
                              >
                                <Settings2 size={16} />
                              </button>
                            </td>
                          </tr>
                        ))}
                      </tbody>
                    </>
                  )}
                  {page === "groups" && (
                    <>
                      <thead>
                        <tr>
                          <th>GROUP</th>
                          {admin && (
                            <>
                              <th>PEOPLE</th>
                              <th>DEVICES</th>
                              <th />
                            </>
                          )}
                        </tr>
                      </thead>
                      <tbody>
                        {(rows as Group[]).map((g) => (
                          <tr key={g.id}>
                            <td>
                              <div className="device-cell">
                                <span className="device-icon">
                                  <Folder size={20} />
                                </span>
                                <strong>{g.name}</strong>
                              </div>
                            </td>
                            {admin && (
                              <>
                                <td>{g.user_ids.length}</td>
                                <td>{g.device_ids.length}</td>
                                <td>
                                  <div className="row-actions">
                                    <button
                                      className="row-edit"
                                      aria-label={"Edit " + g.name}
                                      onClick={() =>
                                        edit({ kind: "group", item: g })
                                      }
                                    >
                                      <Settings2 size={16} />
                                    </button>
                                    <button
                                      className="text-button danger"
                                      onClick={() =>
                                        edit({
                                          kind: "confirm",
                                          title: "Remove group?",
                                          message: `Remove “${g.name}” and its access rules? Accounts and devices will remain.`,
                                          run: () =>
                                            request(
                                              "/groups/" + g.id,
                                              "DELETE",
                                            ),
                                        })
                                      }
                                    >
                                      Remove
                                    </button>
                                  </div>
                                </td>
                              </>
                            )}
                          </tr>
                        ))}
                      </tbody>
                    </>
                  )}
                  {page === "grants" && (
                    <>
                      <thead>
                        <tr>
                          <th>PEOPLE IN</th>
                          <th>CAN</th>
                          <th>DEVICES IN</th>
                          <th />
                        </tr>
                      </thead>
                      <tbody>
                        {(rows as Grant[]).map((g) => (
                          <tr key={g.id}>
                            <td>
                              <strong>{groupName(g.subject_group)}</strong>
                            </td>
                            <td>
                              <Badge
                                tone={
                                  g.capability === "connect"
                                    ? "green"
                                    : "neutral"
                                }
                              >
                                {g.capability === "connect"
                                  ? "View & connect"
                                  : "View only"}
                              </Badge>
                            </td>
                            <td>
                              <strong>{groupName(g.target_group)}</strong>
                            </td>
                            <td>
                              <button
                                className="text-button danger"
                                onClick={() =>
                                  edit({
                                    kind: "confirm",
                                    title: "Revoke access rule?",
                                    message:
                                      "This will block new connections that depend on this rule. Existing sessions are not disconnected.",
                                    run: () =>
                                      request("/grants/" + g.id, "DELETE"),
                                  })
                                }
                              >
                                Revoke
                              </button>
                            </td>
                          </tr>
                        ))}
                      </tbody>
                    </>
                  )}
                  {page === "audit" && (
                    <>
                      <thead>
                        <tr>
                          <th>TIME</th>
                          <th>ACTION / ACTOR</th>
                          <th>TARGET</th>
                          <th>OUTCOME</th>
                          <th>SOURCE</th>
                        </tr>
                      </thead>
                      <tbody>
                        {(rows as Audit[]).map((a) => (
                          <tr key={a.id}>
                            <td className="muted nowrap">{time(a.time)}</td>
                            <td>
                              <strong>{a.action}</strong>
                              <span className="subline">{a.actor}</span>
                            </td>
                            <td>
                              <code title={a.detail}>
                                {a.target.length > 24
                                  ? a.target.slice(0, 20) + "…"
                                  : a.target}
                              </code>
                            </td>
                            <td>
                              <Badge
                                tone={
                                  a.outcome === "denied"
                                    ? "red"
                                    : a.outcome === "allowed"
                                      ? "green"
                                      : "neutral"
                                }
                              >
                                {a.outcome}
                              </Badge>
                            </td>
                            <td className="muted">{a.source}</td>
                          </tr>
                        ))}
                      </tbody>
                    </>
                  )}
                </table>
              </div>
            )}
            <footer className="table-footer">
              <span>
                {page === "overview"
                  ? `Showing up to 5 of ${total} devices`
                  : `${total} record${total === 1 ? "" : "s"} · Page ${number}`}
              </span>
              {page !== "overview" && (
                <div className="pagination">
                  <button
                    aria-label="Previous page"
                    disabled={number === 1 || busy}
                    onClick={() => setNumber((n) => n - 1)}
                  >
                    <ChevronLeft size={17} />
                  </button>
                  <button
                    aria-label="Next page"
                    disabled={number * 25 >= total || busy}
                    onClick={() => setNumber((n) => n + 1)}
                  >
                    <ChevronRight size={17} />
                  </button>
                </div>
              )}
              <span className="footer-live">
                <span />
                Private management service
              </span>
            </footer>
          </section>
          <div className="page-footer">
            <span>Relivo Console</span>
            <span>Self-hosted · Managed access</span>
          </div>
        </main>
      </div>
      {notice && (
        <div className="toast" role="status">
          <Check size={17} />
          {notice}
          <button aria-label="Dismiss message" onClick={() => setNotice("")}>
            <X size={14} />
          </button>
        </div>
      )}
      {editor && (
        <EditorDialog
          editor={editor}
          groups={groupOptions}
          people={peopleOptions}
          devices={deviceOptions}
          close={() => setEditor(null)}
          done={done}
          credential={(credential, id) => {
            setEditor({ kind: "credential", credential, id });
            setRefresh((v) => v + 1);
          }}
        />
      )}
    </div>
  );
}
function Stat({
  label,
  value,
  description,
  icon,
  accent = false,
}: {
  label: string;
  value?: number | string;
  description: string;
  icon: ReactNode;
  accent?: boolean;
}) {
  return (
    <div className={"stat " + (accent ? "accent" : "")}>
      <div className="stat-top">
        <span>{label}</span>
        {icon}
      </div>
      <div className="stat-value">{value ?? "—"}</div>
      <div className="stat-description">
        {accent && <span className="dot" />}
        {description}
      </div>
    </div>
  );
}
function Login({ onLogin }: { onLogin: (u: User, c: string) => void }) {
  const [error, setError] = useState(""),
    [busy, setBusy] = useState(false);
  async function submit(e: FormEvent<HTMLFormElement>) {
    e.preventDefault();
    setBusy(true);
    setError("");
    const data = new FormData(e.currentTarget);
    try {
      const result = await request<{ user: User; csrf: string }>(
        "/login",
        "POST",
        { username: data.get("username"), password: data.get("password") },
      );
      onLogin(result.user, result.csrf);
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  }
  return (
    <div className="login">
      <div className="login-story">
        <div className="brand">
          <span className="brand-icon">
            <ArrowDownLeft size={18} />
            <ArrowUpRight size={18} />
          </span>
          <span>
            Relivo<span className="brand-sub">CONSOLE</span>
          </span>
        </div>
        <div className="login-copy">
          <div className="eyebrow">YOUR PRIVATE WORKSPACE</div>
          <h1>
            Every device.
            <br />
            The right access.
            <br />
            <em>One clear view.</em>
          </h1>
          <p>
            Manage your remote devices and your people,
            <br />
            on infrastructure you control.
          </p>
          <div className="login-lines">
            <span>
              <ShieldCheck size={18} />
              Explicit access permissions
            </span>
            <span>
              <Activity size={18} />A traceable activity record
            </span>
            <span>
              <Globe size={18} />
              Hosted on your own server
            </span>
          </div>
        </div>
        <div className="login-foot">Your devices. Your infrastructure.</div>
      </div>
      <div className="login-form-area">
        <form className="login-form" onSubmit={submit}>
          <span className="login-lock">
            <LockKeyhole size={24} />
          </span>
          <div className="eyebrow">WELCOME BACK</div>
          <h2>Open your workspace</h2>
          <p>Sign in with your administrator or team account.</p>
          {error && (
            <div className="alert" role="alert">
              {error}
            </div>
          )}
          <label>
            Username
            <input
              name="username"
              autoComplete="username"
              autoFocus
              required
              maxLength={64}
              placeholder="Your username"
            />
          </label>
          <label>
            Password
            <input
              name="password"
              type="password"
              autoComplete="current-password"
              required
              maxLength={128}
              placeholder="Your password"
            />
          </label>
          <button disabled={busy} className="button primary">
            {busy ? (
              <LoaderCircle className="spin" size={16} />
            ) : (
              <>
                Sign in <ArrowRight size={17} />
              </>
            )}
          </button>
          <div className="login-help">
            Need an account? Contact your workspace administrator.
          </div>
        </form>
        <div className="login-caption">
          <Shield size={13} /> Access is limited to authorized members.
        </div>
      </div>
    </div>
  );
}

function EditorDialog({
  editor,
  groups,
  people,
  devices,
  close,
  done,
  credential,
}: {
  editor: Editor;
  groups: Group[];
  people: User[];
  devices: Device[];
  close: () => void;
  done: (message?: string) => void;
  credential: (key: string, id: string) => void;
}) {
  const ref = useRef<HTMLDialogElement>(null),
    [error, setError] = useState(""),
    [busy, setBusy] = useState(false),
    [copied, setCopied] = useState(false);
  useEffect(() => {
    ref.current?.showModal();
    return () => ref.current?.close();
  }, []);
  const editing = "item" in editor && editor.item;
  const title =
    editor.kind === "credential"
      ? "Save the enrollment credential"
      : editor.kind === "confirm"
        ? editor.title
        : editor.kind === "grant"
          ? "Add an access rule"
          : `${editing ? "Edit" : "Add"} ${editor.kind === "user" ? "person" : editor.kind}`;
  async function submit(e: FormEvent<HTMLFormElement>) {
    e.preventDefault();
    setBusy(true);
    setError("");
    const f = new FormData(e.currentTarget),
      str = (k: string) => String(f.get(k) || "");
    try {
      if (editor.kind === "user") {
        await request(
          "/users" + (editor.item ? "/" + editor.item.id : ""),
          editor.item ? "PUT" : "POST",
          {
            username: str("username"),
            display_name: str("display_name"),
            role: str("role"),
            enabled: f.get("enabled") === "on",
            password: str("password") || undefined,
          },
        );
        done();
      } else if (editor.kind === "device") {
        const result = await request<{ id: string; credential?: string }>(
          "/devices" + (editor.item ? "/" + editor.item.id : ""),
          editor.item ? "PUT" : "POST",
          {
            id: str("id"),
            name: str("name"),
            platform: str("platform"),
            note: str("note"),
            state: str("state") || "pending",
          },
        );
        if (result.credential) credential(result.credential, result.id);
        else done();
      } else if (editor.kind === "group") {
        await request(
          "/groups" + (editor.item ? "/" + editor.item.id : ""),
          editor.item ? "PUT" : "POST",
          {
            name: str("name"),
            user_ids: f.getAll("user_ids"),
            device_ids: f.getAll("device_ids"),
          },
        );
        done();
      } else if (editor.kind === "grant") {
        await request("/grants", "POST", {
          subject_group: str("subject_group"),
          target_group: str("target_group"),
          capability: str("capability"),
        });
        done();
      } else if (editor.kind === "confirm") {
        await editor.run();
        done();
      }
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  }
  async function rotate(id: string) {
    if (
      !window.confirm(
        "Rotate this device’s credential? The current credential will stop working and approval will be required again.",
      )
    )
      return;
    setBusy(true);
    try {
      const result = await request<{ credential: string }>(
        `/devices/${id}/credential`,
        "POST",
        {},
      );
      credential(result.credential, id);
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  }
  return (
    <dialog
      ref={ref}
      onCancel={(e) => {
        e.preventDefault();
        if (!busy) close();
      }}
      className="editor"
    >
      <div className="dialog-heading">
        <div>
          <div className="eyebrow">WORKSPACE MANAGEMENT</div>
          <h2>{title}</h2>
        </div>
        <button
          className="row-edit"
          aria-label="Close dialog"
          disabled={busy}
          onClick={close}
        >
          <X size={20} />
        </button>
      </div>
      {error && (
        <div className="alert" role="alert">
          {error}
        </div>
      )}
      {editor.kind === "credential" ? (
        <div className="credential">
          <p>
            This credential is displayed only now. Store it securely and install
            it on device <strong>{editor.id}</strong>.
          </p>
          <div className="secret">
            <code>{editor.credential}</code>
            <button
              className="row-edit"
              aria-label="Copy enrollment credential"
              onClick={() =>
                navigator.clipboard
                  .writeText(editor.credential)
                  .then(() => setCopied(true))
                  .catch(() =>
                    setError(
                      "Copy failed. Select and copy the credential manually.",
                    ),
                  )
              }
            >
              {copied ? <Check size={17} /> : <Copy size={17} />}
            </button>
          </div>
          <p className="helper">
            Use the managed client configuration described in the project
            runbook. After its first verified report, return here to approve the
            device.
          </p>
          <button
            className="button primary"
            onClick={() => done("Device enrollment prepared")}
          >
            I have saved the credential
          </button>
        </div>
      ) : (
        <form onSubmit={submit}>
          {editor.kind === "confirm" && (
            <p className="confirm-copy">{editor.message}</p>
          )}
          {editor.kind === "user" && (
            <>
              <label>
                Display name
                <input
                  name="display_name"
                  defaultValue={editor.item?.display_name}
                  required
                  maxLength={120}
                />
              </label>
              {!editor.item && (
                <label>
                  Username
                  <input
                    name="username"
                    required
                    minLength={3}
                    maxLength={64}
                    pattern="[a-zA-Z0-9._@\-]+"
                    autoComplete="off"
                  />
                </label>
              )}
              <div className="form-row">
                <label>
                  Role
                  <select
                    name="role"
                    defaultValue={editor.item?.role || "operator"}
                  >
                    <option value="operator">Operator</option>
                    <option value="viewer">Viewer</option>
                    <option value="admin">Administrator</option>
                  </select>
                </label>
                <label className="checkbox">
                  <input
                    type="checkbox"
                    name="enabled"
                    defaultChecked={editor.item?.enabled ?? true}
                  />
                  Account enabled
                </label>
              </div>
              <label>
                {editor.item ? "New password (optional)" : "Password"}
                <input
                  type="password"
                  name="password"
                  required={!editor.item}
                  minLength={12}
                  maxLength={128}
                  autoComplete="new-password"
                  placeholder={
                    editor.item
                      ? "Leave blank to keep the current password"
                      : "At least 12 characters"
                  }
                />
              </label>
              <p className="helper">
                Operators need a connect rule. Viewers can browse assigned
                devices. Administrators manage the entire workspace.
              </p>
            </>
          )}
          {editor.kind === "device" && (
            <>
              <label>
                Device name
                <input
                  name="name"
                  defaultValue={editor.item?.name}
                  required
                  maxLength={120}
                  placeholder="e.g. Reception workstation"
                />
              </label>
              <div className="form-row">
                <label>
                  Device ID
                  <input
                    name="id"
                    defaultValue={editor.item?.id}
                    readOnly={!!editor.item}
                    required
                    minLength={6}
                    maxLength={253}
                    placeholder="123456789"
                  />
                </label>
                <label>
                  Platform
                  <input
                    name="platform"
                    defaultValue={editor.item?.platform}
                    maxLength={120}
                    placeholder="Reported by the device"
                  />
                </label>
              </div>
              {editor.item && (
                <label>
                  Enrollment state
                  <select name="state" defaultValue={editor.item.state}>
                    <option value="pending">Pending review</option>
                    <option value="approved" disabled={!editor.item.verified}>
                      Approved
                    </option>
                    <option value="disabled">Disabled</option>
                  </select>
                </label>
              )}
              <label>
                Notes
                <textarea
                  name="note"
                  defaultValue={editor.item?.note}
                  maxLength={512}
                  rows={3}
                />
              </label>
              {editor.item ? (
                <>
                  <p className="helper">
                    {editor.item.verified
                      ? "The device has reported using its enrollment credential. Confirm its identity before approving."
                      : "Awaiting a verified device report. Approval is currently unavailable."}
                  </p>
                  {editor.item.public_key && (
                    <p className="key-fingerprint">
                      <strong>Public key</strong>
                      <code>{editor.item.public_key}</code>
                    </p>
                  )}
                  <button
                    type="button"
                    className="text-button danger"
                    disabled={busy}
                    onClick={() => editor.item && rotate(editor.item.id)}
                  >
                    <KeyRound size={14} />
                    Rotate enrollment credential
                  </button>
                </>
              ) : (
                <p className="helper">
                  A private enrollment credential will be generated. New devices
                  start pending and cannot receive managed connections.
                </p>
              )}
            </>
          )}
          {editor.kind === "group" && (
            <>
              <label>
                Group name
                <input
                  name="name"
                  defaultValue={editor.item?.name}
                  required
                  maxLength={80}
                  placeholder="e.g. Support team"
                />
              </label>
              <div className="form-row membership">
                <fieldset>
                  <legend>People</legend>
                  {people.length === 0 ? (
                    <p>No people yet.</p>
                  ) : (
                    people.map((u) => (
                      <label className="checkbox" key={u.id}>
                        <input
                          type="checkbox"
                          name="user_ids"
                          value={u.id}
                          defaultChecked={editor.item?.user_ids.includes(u.id)}
                        />
                        {u.display_name}
                        <small>{u.role}</small>
                      </label>
                    ))
                  )}
                </fieldset>
                <fieldset>
                  <legend>Devices</legend>
                  {devices.length === 0 ? (
                    <p>No devices yet.</p>
                  ) : (
                    devices.map((d) => (
                      <label className="checkbox" key={d.id}>
                        <input
                          type="checkbox"
                          name="device_ids"
                          value={d.id}
                          defaultChecked={editor.item?.device_ids.includes(
                            d.id,
                          )}
                        />
                        {d.name}
                      </label>
                    ))
                  )}
                </fieldset>
              </div>
              <p className="helper">
                Membership organizes your workspace. An access rule is required
                to give members visibility or connection permission.
              </p>
            </>
          )}
          {editor.kind === "grant" && (
            <>
              <label>
                People in
                <select name="subject_group" required defaultValue="">
                  <option value="" disabled>
                    Select a group
                  </option>
                  {groups.map((g) => (
                    <option key={g.id} value={g.id}>
                      {g.name}
                    </option>
                  ))}
                </select>
              </label>
              <label>
                Can
                <select name="capability" defaultValue="view">
                  <option value="view">View devices</option>
                  <option value="connect">View and connect to devices</option>
                </select>
              </label>
              <label>
                Devices in
                <select name="target_group" required defaultValue="">
                  <option value="" disabled>
                    Select a device group
                  </option>
                  {groups.map((g) => (
                    <option key={g.id} value={g.id}>
                      {g.name}
                    </option>
                  ))}
                </select>
              </label>
              <p className="helper">
                A viewer remains read-only even when their group has a connect
                rule. Access changes apply to new connections.
              </p>
              {groups.length === 0 && (
                <div className="alert">
                  Create a group before assigning access.
                </div>
              )}
            </>
          )}
          <div className="dialog-footer">
            <button
              type="button"
              className="button secondary"
              disabled={busy}
              onClick={close}
            >
              Cancel
            </button>
            <button
              className={
                "button " +
                (editor.kind === "confirm" ? "destructive" : "primary")
              }
              disabled={
                busy || (editor.kind === "grant" && groups.length === 0)
              }
            >
              {busy ? (
                <LoaderCircle size={16} className="spin" />
              ) : editor.kind === "confirm" ? (
                "Confirm"
              ) : editing ? (
                "Save changes"
              ) : (
                "Create"
              )}
            </button>
          </div>
        </form>
      )}
    </dialog>
  );
}

const root = document.getElementById("root");
if (root)
  createRoot(root).render(
    <React.StrictMode>
      <App />
    </React.StrictMode>,
  );
