import type { ApiIdResponse, ApiResponse } from "./index";
import { GhostTeamClient } from "./index";

export interface Task {
  id: number;
  creator: string;
  assignee?: string | null;
  description: string;
  status: string;
  result?: string | null;
  created_at?: string | null;
  updated_at?: string | null;
}

export interface TaskHistory {
  id: number;
  task_id: number;
  event: string;
  actor: string;
  at?: string | null;
}

export interface TaskDetails {
  task: Task;
  history: TaskHistory[];
}

export interface CreateTaskRequest {
  from: string;
  to: string;
  description: string;
}

export interface AckTaskRequest {
  id: number;
  worker: string;
}

export interface CompleteTaskRequest {
  id: number;
  worker: string;
  result: string;
}

export interface RequeueTaskRequest {
  id: number;
}

export interface TaskStatusResponse {
  ok: boolean;
  id: number;
}

export type TaskCreateResponse =
  | ApiResponse<TaskDetails>
  | ApiIdResponse<number>;

export async function listTasks(client: GhostTeamClient): Promise<Task[]> {
  const response = await client.get<ApiResponse<Task[]>>("/tasks");
  return response.data;
}

export async function createTask(
  client: GhostTeamClient,
  request: CreateTaskRequest,
): Promise<TaskCreateResponse> {
  return client.post<TaskCreateResponse>("/tasks/create", request);
}

export async function ackTask(
  client: GhostTeamClient,
  request: AckTaskRequest,
): Promise<TaskStatusResponse> {
  return client.post<TaskStatusResponse>("/tasks/ack", request);
}

export async function completeTask(
  client: GhostTeamClient,
  request: CompleteTaskRequest,
): Promise<TaskStatusResponse> {
  return client.post<TaskStatusResponse>("/tasks/complete", request);
}

export async function requeueTask(
  client: GhostTeamClient,
  request: RequeueTaskRequest,
): Promise<TaskStatusResponse> {
  return client.post<TaskStatusResponse>("/tasks/requeue", request);
}

export async function getTask(
  client: GhostTeamClient,
  id: number,
): Promise<TaskDetails | null> {
  try {
    const response = await client.get<ApiResponse<TaskDetails>>(`/tasks/${id}`);
    return response.data;
  } catch (error) {
    if (error instanceof Error && "status" in error && (error as { status?: number }).status === 404) {
      return null;
    }
    throw error;
  }
}
