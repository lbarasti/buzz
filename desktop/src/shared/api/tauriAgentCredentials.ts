import { invokeTauri } from "@/shared/api/tauri";

/**
 * Status of a persona's isolated opencode credential store. Provider ids and
 * directory paths only — API keys never round-trip back to the UI.
 */
export type PersonaOpencodeCredentialState = {
  /** Whether the persona opted into isolated credentials (machine-local). */
  enabled: boolean;
  provisioned: boolean;
  providers: string[];
  /** Isolated opencode data root (becomes `XDG_DATA_HOME` at spawn). */
  dataDir: string;
  /** Isolated opencode config root (becomes `XDG_CONFIG_HOME` at spawn). */
  configDir: string;
};

export async function getPersonaOpencodeCredentials(
  personaId: string,
): Promise<PersonaOpencodeCredentialState> {
  return invokeTauri<PersonaOpencodeCredentialState>(
    "get_persona_opencode_credentials",
    { personaId },
  );
}

export async function setPersonaOpencodeCredential(
  personaId: string,
  providerId: string,
  apiKey: string,
): Promise<PersonaOpencodeCredentialState> {
  return invokeTauri<PersonaOpencodeCredentialState>(
    "set_persona_opencode_credential",
    { personaId, providerId, apiKey },
  );
}

export async function setPersonaOpencodeCredentialsEnabled(
  personaId: string,
  enabled: boolean,
): Promise<PersonaOpencodeCredentialState> {
  return invokeTauri<PersonaOpencodeCredentialState>(
    "set_persona_opencode_credentials_enabled",
    { personaId, enabled },
  );
}

export async function clearPersonaOpencodeCredential(
  personaId: string,
  providerId: string,
): Promise<PersonaOpencodeCredentialState> {
  return invokeTauri<PersonaOpencodeCredentialState>(
    "clear_persona_opencode_credential",
    { personaId, providerId },
  );
}
