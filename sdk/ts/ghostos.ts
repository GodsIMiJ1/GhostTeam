import { GhostTeamClient } from "./index";

export interface GhostOsInferRequest {
  prompt: string;
}

export interface GhostOsInferResponse {
  output: string;
}

export async function infer(
  client: GhostTeamClient,
  request: GhostOsInferRequest,
): Promise<GhostOsInferResponse> {
  return client.post<GhostOsInferResponse>("/ghostos/infer", request);
}
