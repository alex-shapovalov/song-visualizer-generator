import { useEffect, useState } from 'react';
import { errorMessage, fetchJobStatus, isFinished, type JobStatus } from './api';

const POLL_INTERVAL_MS = 500;

interface PollState {
  jobId: string;
  status: JobStatus | null;
  error: string | null;
}

interface JobStatusResult {
  status: JobStatus | null;
  error: string | null;
}

export function useJobStatus(jobId: string | null): JobStatusResult {
  const [state, setState] = useState<PollState | null>(null);

  useEffect(() => {
    if (jobId === null) {
      return;
    }
    const controller = new AbortController();
    let timer: ReturnType<typeof setTimeout> | undefined;

    const poll = async () => {
      try {
        const status = await fetchJobStatus(jobId, controller.signal);
        setState({ jobId, status, error: null });
        if (!isFinished(status.stage)) {
          timer = setTimeout(poll, POLL_INTERVAL_MS);
        }
      } catch (caught) {
        if (!controller.signal.aborted) {
          setState({ jobId, status: null, error: errorMessage(caught) });
        }
      }
    };
    void poll();

    return () => {
      controller.abort();
      clearTimeout(timer);
    };
  }, [jobId]);

  if (state === null || state.jobId !== jobId) {
    return { status: null, error: null };
  }
  return { status: state.status, error: state.error };
}
