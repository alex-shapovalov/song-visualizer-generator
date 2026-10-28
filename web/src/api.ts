export type Stage = 'queued' | 'analyzing' | 'rendering' | 'finalizing' | 'done' | 'failed';

export interface JobStatus {
  stage: Stage;
  progress: number;
  error: string | null;
  fileName: string;
}

export interface VisualizerInput {
  title: string;
  artist: string;
  art: File;
  audio: File;
}

export function isFinished(stage: Stage | undefined): boolean {
  return stage === 'done' || stage === 'failed';
}

export async function createJob(input: VisualizerInput): Promise<string> {
  const body = new FormData();
  body.append('title', input.title);
  body.append('artist', input.artist);
  body.append('art', input.art);
  body.append('audio', input.audio);

  const response = await fetch('/api/jobs', { method: 'POST', body });
  const { id } = await parseResponse<{ id: string }>(response);
  return id;
}

export async function fetchJobStatus(id: string, signal: AbortSignal): Promise<JobStatus> {
  const response = await fetch(`/api/jobs/${id}`, { signal });
  return parseResponse<JobStatus>(response);
}

export function videoUrl(id: string): string {
  return `/api/jobs/${id}/video`;
}

export function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

async function parseResponse<T>(response: Response): Promise<T> {
  if (response.ok) {
    return (await response.json()) as T;
  }
  const body = (await response.json().catch(() => null)) as { error?: string } | null;
  throw new Error(body?.error ?? `Request failed with status ${response.status}`);
}
