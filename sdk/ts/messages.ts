import { GhostTeamClient } from "./index";
import type { ApiResponse } from "./index";

export interface Message {
  id: number;
  sender: string;
  recipient: string;
  body: string;
  created_at?: string | null;
  read: number;
}

export interface SendMessageRequest {
  from: string;
  to: string;
  body: string;
}

export interface MarkReadRequest {
  id: number;
}

export interface MessageSendResponse {
  ok: boolean;
  from: string;
  to: string;
}

export interface MessageReadResponse {
  ok: boolean;
  id: number;
}

export async function getUnreadMessages(
  client: GhostTeamClient,
  agent: string,
): Promise<Message[]> {
  const response = await client.get<ApiResponse<Message[]>>(
    `/messages/${encodeURIComponent(agent)}`
  );
  return response.data;
}

export async function sendMessage(
  client: GhostTeamClient,
  request: SendMessageRequest,
): Promise<MessageSendResponse> {
  return client.post<MessageSendResponse>("/messages/send", request);
}

export async function markRead(
  client: GhostTeamClient,
  request: MarkReadRequest,
): Promise<MessageReadResponse> {
  return client.post<MessageReadResponse>("/messages/mark-read", request);
}
