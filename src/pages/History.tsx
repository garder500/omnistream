import { useCallback, useEffect, useMemo, useState } from "react";
import EventFeed, { eventLabel } from "../components/EventFeed";
import { Button, Card, LiveBadge, PageHeader } from "../components/ui";
import { api } from "../lib/api";
import { PLATFORMS, type PlatformKey } from "../lib/platforms";
import { useStore } from "../lib/store";
import type { SessionDetail, SessionSummary, StreamEventType, ViewerPoint } from "../lib/types";

const fmt = (n: number) => n.toLocaleString("fr-FR");
const dateFmt = new Intl.DateTimeFormat("fr-FR", { day: "2-digit", month: "short", hour: "2-digit", minute: "2-digit" });
const timeFmt = new Intl.DateTimeFormat("fr-FR", { hour: "2-digit", minute: "2-digit" });

function duration(ms: number): string {
  const min = Math.max(0, Math.round(ms / 60_000));
  return min < 60 ? `${min} min` : `${Math.floor(min / 60)} h ${String(min % 60).padStart(2, "0")}`;
}

const endOf = (s: SessionSummary) => s.ended_at_ms ?? Date.now();

export default function History() {
  const { notify } = useStore();
  const [sessions, setSessions] = useState<SessionSummary[] | null>(null);
  const [selected, setSelected] = useState<string | null>(null);
  const [detail, setDetail] = useState<SessionDetail | null>(null);

  const load = useCallback(async () => {
    try {
      const list = await api.listSessions();
      setSessions(list);
      setSelected((cur) => (cur && list.some((s) => s.id === cur) ? cur : (list[0]?.id ?? null)));
    } catch (e) {
      setSessions([]);
      notify(String(e), true);
    }
  }, [notify]);

  useEffect(() => {
    load();
  }, [load]);

  useEffect(() => {
    if (!selected) {
      setDetail(null);
      return;
    }
    let cancelled = false;
    api
      .sessionDetail(selected)
      .then((d) => !cancelled && setDetail(d))
      .catch((e) => !cancelled && notify(String(e), true));
    return () => {
      cancelled = true;
    };
  }, [selected, notify]);

  const exportCsv = async () => {
    if (!detail) return;
    try {
      notify(`Export enregistré : ${await api.saveSessionCsv(detail.summary.id)}`);
    } catch (e) {
      notify(String(e), true);
    }
  };

  const remove = async () => {
    if (!detail || !window.confirm("Supprimer définitivement cette session de l'historique ?")) return;
    try {
      await api.deleteSession(detail.summary.id);
      setDetail(null);
      await load();
    } catch (e) {
      notify(String(e), true);
    }
  };

  return (
    <div className="flex h-full flex-col">
      <PageHeader
        title="Historique"
        subtitle="Vos sessions de live : spectateurs, événements et export."
        actions={
          detail && (
            <>
              <Button variant="ghost" onClick={remove}>
                Supprimer
              </Button>
              <Button variant="primary" onClick={exportCsv}>
                Exporter en CSV
              </Button>
            </>
          )
        }
      />

      {sessions === null ? (
        <p className="text-muted">Chargement…</p>
      ) : sessions.length === 0 ? (
        <Card className="m-auto max-w-md p-8 text-center text-muted">
          <p className="text-base font-semibold text-zinc-100">Aucune session enregistrée</p>
          <p className="mt-2">Une session est créée automatiquement dès qu'une de vos chaînes suivies passe en direct.</p>
        </Card>
      ) : (
        <div className="flex min-h-0 flex-1 gap-5">
          <Card className="flex w-72 shrink-0 flex-col gap-1.5 overflow-auto p-2.5">
            {sessions.map((s) => (
              <button
                key={s.id}
                type="button"
                aria-pressed={s.id === selected}
                onClick={() => setSelected(s.id)}
                className={`rounded-control border p-3 text-left transition ${s.id === selected ? "border-brand bg-brand-soft" : "border-transparent hover:bg-raised"}`}
              >
                <div className="flex items-center justify-between gap-2 font-semibold">
                  {dateFmt.format(s.started_at_ms)}
                  {s.ended_at_ms === null && <LiveBadge>EN COURS</LiveBadge>}
                </div>
                <div className="mt-0.5 text-[13px] text-muted">
                  {duration(endOf(s) - s.started_at_ms)} · pic {fmt(s.peak_viewers)} · {s.event_count} évén.
                </div>
              </button>
            ))}
          </Card>

          {detail ? <SessionView detail={detail} /> : <p className="text-muted">Chargement de la session…</p>}
        </div>
      )}
    </div>
  );
}

function SessionView({ detail }: { detail: SessionDetail }) {
  const { summary, points, events } = detail;
  const tiles = [
    { label: "Durée", value: duration(endOf(summary) - summary.started_at_ms), hint: summary.ended_at_ms === null ? "Session en cours" : timeFmt.format(summary.started_at_ms) + " → " + timeFmt.format(summary.ended_at_ms) },
    { label: "Pic de spectateurs", value: fmt(summary.peak_viewers), hint: "Somme brute, doublons inclus" },
    { label: "Moyenne", value: fmt(summary.avg_viewers), hint: `${points.length} relevé${points.length > 1 ? "s" : ""}` },
    { label: "Événements", value: fmt(summary.event_count), hint: Object.entries(summary.events_by_kind).map(([k, n]) => `${n} ${eventLabel(k as StreamEventType, n).toLowerCase()}`).join(" · ") || "Aucun" },
  ];

  return (
    <div className="flex min-w-0 flex-1 flex-col gap-4 overflow-auto">
      <div className="grid grid-cols-4 gap-4">
        {tiles.map((t) => (
          <Card key={t.label} className="p-4">
            <div className="text-muted">{t.label}</div>
            <div className="mt-1 text-2xl font-bold">{t.value}</div>
            <div className="mt-0.5 truncate text-[13px] text-muted" title={t.hint}>
              {t.hint}
            </div>
          </Card>
        ))}
      </div>

      <Card className="p-5">
        <div className="mb-3 font-semibold">Spectateurs au fil du live</div>
        <ViewerChart points={points} />
      </Card>

      <EventFeed events={events} title="Événements de la session" emptyText="Aucun événement pendant cette session." absoluteTime className="min-h-72" />
    </div>
  );
}

const W = 720;
const H = 240;
const PAD = { l: 48, r: 16, t: 14, b: 28 };

/** Courbe simple en SVG : total et une courbe par plateforme. */
function ViewerChart({ points }: { points: ViewerPoint[] }) {
  const [hover, setHover] = useState<number | null>(null);

  const model = useMemo(() => {
    const platforms = [...new Set(points.flatMap((p) => Object.keys(p.by_platform)))].filter((p): p is PlatformKey => p in PLATFORMS);
    const t0 = points[0]?.ts_ms ?? 0;
    const t1 = points[points.length - 1]?.ts_ms ?? 1;
    const maxY = niceMax(Math.max(1, ...points.map((p) => p.total)));
    const x = (ts: number) => PAD.l + (t1 === t0 ? (W - PAD.l - PAD.r) / 2 : ((ts - t0) / (t1 - t0)) * (W - PAD.l - PAD.r));
    const y = (v: number) => PAD.t + (1 - v / maxY) * (H - PAD.t - PAD.b);
    return { platforms, t0, t1, maxY, x, y };
  }, [points]);

  if (points.length === 0) return <p className="py-10 text-center text-muted">Aucun relevé de spectateurs pour cette session.</p>;

  const path = (get: (p: ViewerPoint) => number) => points.map((p, i) => `${i ? "L" : "M"}${model.x(p.ts_ms).toFixed(1)} ${model.y(get(p)).toFixed(1)}`).join(" ");
  const ticks = [0, 0.5, 1].map((f) => Math.round(model.maxY * f));
  const hp = hover !== null ? points[hover] : null;

  const onMove = (e: React.MouseEvent<SVGSVGElement>) => {
    const rect = e.currentTarget.getBoundingClientRect();
    const px = ((e.clientX - rect.left) / rect.width) * W;
    let best = 0;
    points.forEach((p, i) => {
      if (Math.abs(model.x(p.ts_ms) - px) < Math.abs(model.x(points[best].ts_ms) - px)) best = i;
    });
    setHover(best);
  };

  return (
    <div>
      <svg
        viewBox={`0 0 ${W} ${H}`}
        className="h-auto w-full"
        role="img"
        aria-label={`Courbe des spectateurs : pic ${fmt(Math.max(...points.map((p) => p.total)))}, ${points.length} relevés`}
        onMouseMove={onMove}
        onMouseLeave={() => setHover(null)}
      >
        {ticks.map((v) => (
          <g key={v}>
            <line x1={PAD.l} x2={W - PAD.r} y1={model.y(v)} y2={model.y(v)} stroke="var(--color-line)" />
            <text x={PAD.l - 8} y={model.y(v) + 4} textAnchor="end" fontSize="11" fill="var(--color-muted)">
              {fmt(v)}
            </text>
          </g>
        ))}
        <text x={PAD.l} y={H - 8} fontSize="11" fill="var(--color-muted)">
          {timeFmt.format(model.t0)}
        </text>
        <text x={W - PAD.r} y={H - 8} textAnchor="end" fontSize="11" fill="var(--color-muted)">
          {timeFmt.format(model.t1)}
        </text>

        {model.platforms.length > 1 &&
          model.platforms.map((p) => (
            <path key={p} d={path((pt) => pt.by_platform[p] ?? 0)} fill="none" stroke={`var(--color-${p})`} strokeWidth="1.75" strokeLinejoin="round" opacity="0.9" />
          ))}
        <path d={path((pt) => pt.total)} fill="none" stroke="#fafafa" strokeWidth="2.5" strokeLinejoin="round" strokeLinecap="round" />
        {points.length === 1 && <circle cx={model.x(points[0].ts_ms)} cy={model.y(points[0].total)} r="4" fill="#fafafa" />}

        {hp && (
          <g>
            <line x1={model.x(hp.ts_ms)} x2={model.x(hp.ts_ms)} y1={PAD.t} y2={H - PAD.b} stroke="var(--color-line-strong)" />
            <circle cx={model.x(hp.ts_ms)} cy={model.y(hp.total)} r="4.5" fill="#fafafa" stroke="var(--color-ink)" strokeWidth="2" />
          </g>
        )}
      </svg>

      <div className="mt-2 flex flex-wrap items-center gap-x-5 gap-y-1 text-[13px] text-soft" aria-live="polite">
        <span className="flex items-center gap-1.5">
          <i className="inline-block h-1 w-4 rounded bg-zinc-50" /> Total (somme brute)
        </span>
        {model.platforms.length > 1 &&
          model.platforms.map((p) => (
            <span key={p} className="flex items-center gap-1.5">
              <i className="inline-block h-1 w-4 rounded" style={{ background: `var(--color-${p})` }} /> {PLATFORMS[p].label}
            </span>
          ))}
        {hp && (
          <span className="ml-auto text-zinc-100">
            {timeFmt.format(hp.ts_ms)} · <b>{fmt(hp.total)}</b>
            {model.platforms.length > 1 && " (" + model.platforms.map((p) => `${PLATFORMS[p].label} ${fmt(hp.by_platform[p] ?? 0)}`).join(", ") + ")"}
          </span>
        )}
      </div>
    </div>
  );
}

/** Arrondit à une borne d'axe « lisible » (1, 2, 5 × 10^n). */
function niceMax(v: number): number {
  const pow = 10 ** Math.floor(Math.log10(v));
  const f = v / pow;
  return (f <= 1 ? 1 : f <= 2 ? 2 : f <= 5 ? 5 : 10) * pow;
}
