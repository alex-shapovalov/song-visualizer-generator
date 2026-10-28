interface JobProgressProps {
  label: string;
  value?: number;
}

export function JobProgress({ label, value }: JobProgressProps) {
  const percent = value === undefined ? undefined : Math.round(value * 100);

  return (
    <div className="progress" role="status">
      <div className="progress-header">
        <span>{label}</span>
        {percent !== undefined && <span>{percent}%</span>}
      </div>
      <div className="progress-track">
        {percent === undefined && <div className="progress-fill indeterminate" />}
        {percent !== undefined && <div className="progress-fill" style={{ width: `${percent}%` }} />}
      </div>
    </div>
  );
}
