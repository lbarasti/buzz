import * as React from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Loader2, Terminal, Trash2 } from "lucide-react";

import { Input } from "@/shared/ui/input";
import { Switch } from "@/shared/ui/switch";
import {
  clearPersonaOpencodeCredential,
  getPersonaOpencodeCredentials,
  setPersonaOpencodeCredential,
  setPersonaOpencodeCredentialsEnabled,
  type PersonaOpencodeCredentialState,
} from "@/shared/api/tauriAgentCredentials";
import { isOpencodeRuntime } from "./buzzAgentConfig";
import { cn } from "@/shared/lib/cn";
import { PERSONA_FIELD_CONTROL_CLASS } from "./agentConfigOptions";

const credentialStateQueryKey = (personaId: string) => [
  "persona-opencode-credentials",
  personaId,
];

/**
 * "Custom credentials" section for opencode-runtime personas (edit mode only).
 *
 * A pure IPC surface over the persona's isolated opencode auth store: the
 * opt-in toggle and provider keys both persist straight to the persona's
 * machine-local credential store — neither the secrets nor the toggle are
 * part of the (shareable) persona definition, and keys never enter persona
 * state or env_vars.
 *
 * Falls back deliberately: opt-in without a provisioned key keeps the owner's
 * default opencode credentials, so this section is advisory, not a save gate.
 * Changes take effect on the agent's next start.
 */
export function PersonaOpencodeCredentialsSection({
  disabled,
  personaId,
}: {
  disabled: boolean;
  personaId: string;
}) {
  const queryClient = useQueryClient();
  const [providerId, setProviderId] = React.useState("");
  const [apiKey, setApiKey] = React.useState("");
  const [error, setError] = React.useState<string | null>(null);

  const stateQuery = useQuery({
    queryFn: () => getPersonaOpencodeCredentials(personaId),
    queryKey: credentialStateQueryKey(personaId),
  });

  const applyState = React.useCallback(
    (next: PersonaOpencodeCredentialState) => {
      queryClient.setQueryData(credentialStateQueryKey(personaId), next);
      setApiKey("");
      setProviderId("");
      setError(null);
    },
    [personaId, queryClient],
  );

  const setMutation = useMutation({
    mutationFn: (input: { apiKey: string; providerId: string }) =>
      setPersonaOpencodeCredential(personaId, input.providerId, input.apiKey),
    onError: (err: Error) => setError(err.message),
    onSuccess: applyState,
  });
  const toggleMutation = useMutation({
    mutationFn: (next: boolean) =>
      setPersonaOpencodeCredentialsEnabled(personaId, next),
    onError: (err: Error) => setError(err.message),
    onSuccess: applyState,
  });
  const clearMutation = useMutation({
    mutationFn: (providerId: string) =>
      clearPersonaOpencodeCredential(personaId, providerId),
    onError: (err: Error) => setError(err.message),
    onSuccess: applyState,
  });

  const state = stateQuery.data;
  const busy =
    setMutation.isPending ||
    clearMutation.isPending ||
    toggleMutation.isPending;
  const enabled = state?.enabled ?? false;

  function handleSave() {
    if (!providerId.trim() || !apiKey.trim()) {
      setError("Enter both a provider id and an API key.");
      return;
    }
    setMutation.mutate({ apiKey, providerId: providerId.trim() });
  }

  return (
    <div className="space-y-3" data-testid="persona-opencode-credentials">
      <div className="flex items-start justify-between gap-4">
        <div className="space-y-1">
          <label
            className="text-sm font-medium text-foreground"
            htmlFor="persona-custom-credentials"
          >
            Custom credentials
          </label>
          <p className="text-xs text-muted-foreground">
            Give this agent its own API keys instead of yours. Without a key
            here, the agent uses your default credentials.
          </p>
        </div>
        <Switch
          checked={enabled}
          disabled={disabled || stateQuery.isPending || busy}
          id="persona-custom-credentials"
          onCheckedChange={(next) => {
            toggleMutation.mutate(next);
          }}
        />
      </div>

      {enabled ? (
        <div className="space-y-3 rounded-md border border-border p-3">
          {state ? (
            state.providers.length > 0 ? (
              <ul className="space-y-1.5">
                {state.providers.map((provider) => (
                  <li
                    className="flex items-center justify-between gap-2"
                    key={provider}
                  >
                    <span
                      className="font-mono text-xs text-foreground"
                      data-testid="persona-opencode-provider"
                    >
                      {provider}
                    </span>
                    <button
                      aria-label={`Remove ${provider} credential`}
                      className="shrink-0 text-muted-foreground hover:text-foreground disabled:opacity-50"
                      disabled={disabled || busy}
                      onClick={() => clearMutation.mutate(provider)}
                      type="button"
                    >
                      {clearMutation.isPending &&
                      clearMutation.variables === provider ? (
                        <Loader2 className="h-4 w-4 animate-spin" />
                      ) : (
                        <Trash2 className="h-4 w-4" />
                      )}
                    </button>
                  </li>
                ))}
              </ul>
            ) : (
              <p className="text-xs text-muted-foreground">
                No custom keys yet — this agent runs on your default credentials
                until you add one.
              </p>
            )
          ) : stateQuery.isError ? (
            <p className="text-xs text-destructive">
              Could not load credential state: {String(stateQuery.error)}
            </p>
          ) : (
            <Loader2 className="h-4 w-4 animate-spin text-muted-foreground" />
          )}

          <div className="grid gap-2 sm:grid-cols-[10rem_1fr_auto]">
            <Input
              autoCapitalize="none"
              autoCorrect="off"
              className={cn("h-8", PERSONA_FIELD_CONTROL_CLASS)}
              disabled={disabled || busy}
              onChange={(event) => setProviderId(event.target.value)}
              placeholder="provider (e.g. anthropic)"
              spellCheck={false}
              value={providerId}
            />
            <Input
              autoComplete="off"
              className={cn("h-8", PERSONA_FIELD_CONTROL_CLASS)}
              disabled={disabled || busy}
              onChange={(event) => setApiKey(event.target.value)}
              placeholder="API key"
              type="password"
              value={apiKey}
            />
            <button
              className={cn(
                "h-8 shrink-0 rounded-md border border-border px-3 text-xs text-foreground",
                "hover:bg-accent disabled:opacity-50",
              )}
              disabled={disabled || busy}
              onClick={handleSave}
              type="button"
            >
              {setMutation.isPending ? (
                <Loader2 className="h-4 w-4 animate-spin" />
              ) : (
                "Save key"
              )}
            </button>
          </div>

          {error ? <p className="text-xs text-destructive">{error}</p> : null}

          <p className="text-xs text-muted-foreground">
            Takes effect on the agent&apos;s next start.
          </p>

          {state ? (
            <details className="text-xs text-muted-foreground">
              <summary className="inline-flex cursor-pointer items-center gap-1.5 hover:text-foreground">
                <Terminal className="h-3.5 w-3.5" />
                Provision in a terminal (OAuth providers)
              </summary>
              <pre className="mt-2 overflow-x-auto rounded-md bg-muted p-2 font-mono text-3xs">
                {`XDG_DATA_HOME='${state.dataDir}' \\\nXDG_CONFIG_HOME='${state.configDir}' \\\nopencode auth login`}
              </pre>
            </details>
          ) : null}
        </div>
      ) : null}
    </div>
  );
}

/**
 * Render gate for the credentials section: only opencode-runtime personas in
 * edit mode (a persona must exist before anything can be provisioned for it)
 * show provisioning controls.
 */
export function PersonaOpencodeCredentialsSlot({
  disabled,
  personaId,
  runtime,
}: {
  disabled: boolean;
  personaId: string | null;
  runtime: string;
}) {
  if (personaId === null || !isOpencodeRuntime(runtime)) return null;
  return (
    <PersonaOpencodeCredentialsSection
      disabled={disabled}
      personaId={personaId}
    />
  );
}
