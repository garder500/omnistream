import { useEffect, useRef, useState } from "react";
import { api } from "../lib/api";
import { PlatformBadge, platformKey } from "../lib/platforms";
import { useStore } from "../lib/store";
import type { PlatformUpdateResult, StreamPreset } from "../lib/types";
import { Button, Card, CardHeader, Field, Input, Select } from "./ui";

const parseTags = (raw: string) =>
  raw
    .split(/[,\s]+/)
    .map((t) => t.trim().replace(/^#/, ""))
    .filter(Boolean);

/** Titre / catégorie / tags : un clic applique à toutes les chaînes, avec le statut de chacune. */
export function StreamInfoForm() {
  const { channels, notify } = useStore();
  const [title, setTitle] = useState("");
  const [category, setCategory] = useState("");
  const [tags, setTags] = useState("");
  const [suggestions, setSuggestions] = useState<string[]>([]);
  const [presets, setPresets] = useState<StreamPreset[]>([]);
  const [presetId, setPresetId] = useState("");
  const [busy, setBusy] = useState(false);
  const [results, setResults] = useState<PlatformUpdateResult[]>([]);
  const timer = useRef<number | undefined>(undefined);

  const refreshPresets = () => api.listPresets().then(setPresets).catch(() => {});
  useEffect(() => {
    refreshPresets();
  }, []);

  useEffect(() => {
    window.clearTimeout(timer.current);
    if (category.trim().length < 2) {
      setSuggestions([]);
      return;
    }
    let stale = false;
    timer.current = window.setTimeout(() => {
      api
        .searchCategories(category.trim())
        .then((c) => !stale && setSuggestions(c.map((x) => x.name)))
        .catch(() => !stale && setSuggestions([]));
    }, 300);
    return () => {
      stale = true;
      window.clearTimeout(timer.current);
    };
  }, [category]);

  const pickPreset = (id: string) => {
    setPresetId(id);
    const p = presets.find((x) => x.id === id);
    if (p) {
      setTitle(p.title);
      setCategory(p.category);
      setTags(p.tags.join(", "));
    }
  };

  const savePreset = async (asNew = false) => {
    const existing = asNew ? undefined : presets.find((p) => p.id === presetId);
    const name = existing?.name ?? window.prompt("Nom du préréglage ?")?.trim();
    if (!name) return;
    const preset: StreamPreset = { id: existing?.id ?? crypto.randomUUID(), name, title, category, tags: parseTags(tags) };
    try {
      await api.savePreset(preset);
      await refreshPresets();
      setPresetId(preset.id);
      notify(`Préréglage « ${name} » enregistré`);
    } catch (e) {
      notify(String(e), true);
    }
  };

  const removePreset = async () => {
    if (!presetId || !window.confirm("Supprimer ce préréglage ?")) return;
    await api.deletePreset(presetId).catch((e) => notify(String(e), true));
    setPresetId("");
    refreshPresets();
  };

  const apply = async () => {
    setBusy(true);
    setResults([]);
    try {
      const res = await api.updateStreamInfo([], {
        title: title.trim() || null,
        category: category.trim() || null,
        tags: tags.trim() ? parseTags(tags) : null,
      });
      setResults(res);
    } catch (e) {
      notify(String(e), true);
    } finally {
      setBusy(false);
    }
  };

  const nameOf = (id: string) => channels.find((c) => c.id === id)?.display_name ?? id;
  const nothing = !title.trim() && !category.trim() && !tags.trim();

  return (
    <Card>
      <CardHeader title="Titre et catégorie" aside={`${channels.length} chaîne${channels.length > 1 ? "s" : ""}`} />
      <div className="flex flex-col gap-4 p-5">
        <div className="flex items-end gap-3">
          <div className="flex-1">
            <Field label="Préréglage">
              <Select value={presetId} onChange={(e) => pickPreset(e.target.value)}>
                <option value="">— Aucun —</option>
                {presets.map((p) => (
                  <option key={p.id} value={p.id}>
                    {p.name}
                  </option>
                ))}
              </Select>
            </Field>
          </div>
          <Button variant="outline" onClick={() => savePreset()} disabled={nothing}>
            {presetId ? "Mettre à jour" : "Enregistrer"}
          </Button>
          {presetId && (
            <Button variant="outline" onClick={() => savePreset(true)} disabled={nothing}>
              Enregistrer sous…
            </Button>
          )}
          {presetId && (
            <Button variant="ghost" onClick={removePreset}>
              Supprimer
            </Button>
          )}
        </div>
        <Field label="Titre du live">
          <Input value={title} maxLength={140} onChange={(e) => setTitle(e.target.value)} placeholder="Ex. Reprise de la campagne, on papote !" />
        </Field>
        <div className="grid grid-cols-2 gap-4">
          <Field label="Catégorie">
            <Input value={category} list="category-suggestions" onChange={(e) => setCategory(e.target.value)} placeholder="Ex. Just Chatting" />
            <datalist id="category-suggestions">
              {suggestions.map((s) => (
                <option key={s} value={s} />
              ))}
            </datalist>
          </Field>
          <Field label="Tags" hint="Séparés par des virgules (10 max)">
            <Input value={tags} onChange={(e) => setTags(e.target.value)} placeholder="Français, chill" />
          </Field>
        </div>
        <div className="flex items-center gap-4">
          <Button variant="primary" onClick={apply} disabled={busy || nothing || channels.length === 0}>
            {busy ? "Envoi…" : "Appliquer à toutes les plateformes"}
          </Button>
          {channels.length === 0 && <span className="text-muted">Ajoutez une chaîne pour commencer.</span>}
        </div>
        {results.length > 0 && (
          <ul className="flex flex-col gap-2" aria-label="Résultat par plateforme">
            {results.map((r) => (
              <li key={r.channel_id} className="flex items-start gap-3 rounded-control border border-line bg-field px-3.5 py-2.5">
                <PlatformBadge platform={platformKey(r.platform)} size={24} />
                <div className="flex-1">
                  <div className="font-medium">{nameOf(r.channel_id)}</div>
                  {r.error && <div className="text-[13px] text-danger-text">{r.error}</div>}
                </div>
                <span className={`text-sm font-semibold ${r.ok ? "text-success" : "text-danger-text"}`}>{r.ok ? "Mis à jour" : "Échec"}</span>
              </li>
            ))}
          </ul>
        )}
      </div>
    </Card>
  );
}
