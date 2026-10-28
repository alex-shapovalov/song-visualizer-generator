import { useState } from 'react';
import { createJob, errorMessage, isFinished, videoUrl, type JobStatus, type Stage, type VisualizerInput } from './api';
import { JobProgress } from './components/JobProgress';
import { VideoResult } from './components/VideoResult';
import { VisualizerForm } from './components/VisualizerForm';
import { useJobStatus } from './useJobStatus';

const STAGE_LABELS: Record<Stage, string> = {
  queued: 'Waiting for the previous render…',
  analyzing: 'Analyzing audio and artwork…',
  rendering: 'Rendering frames…',
  finalizing: 'Finalizing video…',
  done: 'Done',
  failed: 'Failed',
};

const DETERMINATE_STAGES: ReadonlySet<Stage> = new Set(['rendering', 'finalizing']);

export function App() {
  const [jobId, setJobId] = useState<string | null>(null);
  const [isUploading, setIsUploading] = useState(false);
  const [submitError, setSubmitError] = useState<string | null>(null);
  const { status, error: pollError } = useJobStatus(jobId);

  const isRenderActive = jobId !== null && pollError === null && !isFinished(status?.stage);
  const visibleError = submitError ?? pollError ?? failureMessage(status);

  async function handleSubmit(input: VisualizerInput) {
    setSubmitError(null);
    setJobId(null);
    setIsUploading(true);
    try {
      setJobId(await createJob(input));
    } catch (caught) {
      setSubmitError(errorMessage(caught));
    } finally {
      setIsUploading(false);
    }
  }

  return (
    <main className="page">
      <header className="header">
        <h1>Song Visualizer</h1>
        <p>Turn a track into a 1080p video, ready to upload to YouTube.</p>
      </header>

      <section className="card">
        <VisualizerForm disabled={isUploading || isRenderActive} onSubmit={handleSubmit} />

        {isUploading && <JobProgress label="Uploading files…" />}

        {status && !isFinished(status.stage) && (
          <JobProgress
            label={STAGE_LABELS[status.stage]}
            value={DETERMINATE_STAGES.has(status.stage) ? status.progress : undefined}
          />
        )}

        {jobId && status?.stage === 'done' && <VideoResult src={videoUrl(jobId)} fileName={status.fileName} />}

        {visibleError && (
          <p className="error" role="alert">
            {visibleError}
          </p>
        )}
      </section>
    </main>
  );
}

function failureMessage(status: JobStatus | null): string | null {
  if (status?.stage !== 'failed') {
    return null;
  }
  return status.error ?? 'Rendering failed';
}
