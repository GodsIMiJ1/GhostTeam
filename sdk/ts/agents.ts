import type { ApiIdResponse, ApiResponse } from "./index";
import { GhostTeamClient } from "./index";

export interface Agent {
  id: string;
  role: string;
  backend: string;
  joined_at?: string | null;
}

export interface JoinAgentRequest {
  id: string;
  role: string;
  backend: string;
}

export interface LeaveAgentRequest {
  id: string;
}

export interface LeaveAgentResponse {
  ok: boolean;
  id: string;
}

export type JoinAgentResponse =
  | ApiResponse<Agent>
  | ApiIdResponse<string>;

export async function listAgents(client: GhostTeamClient): Promise<Agent[]> {
  const response = await client.get<ApiResponse<Agent[]>>("/agents");
  return response.data;
}

export async function joinAgent(
  client: GhostTeamClient,
  request: JoinAgentRequest,
): Promise<JoinAgentResponse> {
  return client.post<JoinAgentResponse>("/agents/join", request);
}

export async function leaveAgent(
  client: GhostTeamClient,
  request: LeaveAgentRequest,
): Promise<LeaveAgentResponse> {
  return client.post<LeaveAgentResponse>("/agents/leave", request);
}

export async function getAgent(
  client: GhostTeamClient,
  id: string,
): Promise<Agent | null> {
  try {
    const response = await client.get<ApiResponse<Agent>>(`/agents/${encodeURIComponent(id)}`);
    return response.data;
  } catch (error) {
    if (error instanceof Error && "status" in error && (error as { status?: number }).status === 404) {
      return null;
    }
    throw error;
  }
}
