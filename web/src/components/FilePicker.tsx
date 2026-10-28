import { useEffect, useState, type ChangeEvent } from 'react';

interface FilePickerProps {
  label: string;
  hint: string;
  accept: string;
  file: File | null;
  showPreview?: boolean;
  onChange: (file: File) => void;
}

export function FilePicker({ label, hint, accept, file, showPreview = false, onChange }: FilePickerProps) {
  const previewUrl = useObjectUrl(showPreview ? file : null);

  function handleChange(event: ChangeEvent<HTMLInputElement>) {
    const selected = event.target.files?.[0];
    if (selected) {
      onChange(selected);
    }
  }

  return (
    <label className="picker">
      <span className="picker-label">{label}</span>
      <div className="picker-zone">
        {previewUrl && <img className="picker-preview" src={previewUrl} alt="" />}
        {!previewUrl && (
          <div className="picker-body">
            <strong>{file ? file.name : 'Choose or drop a file'}</strong>
            <span>{file ? formatSize(file.size) : hint}</span>
          </div>
        )}
        <input type="file" accept={accept} onChange={handleChange} />
      </div>
    </label>
  );
}

function useObjectUrl(file: File | null): string | null {
  const [url, setUrl] = useState<string | null>(null);

  useEffect(() => {
    if (file === null) {
      setUrl(null);
      return;
    }
    const objectUrl = URL.createObjectURL(file);
    setUrl(objectUrl);
    return () => URL.revokeObjectURL(objectUrl);
  }, [file]);

  return url;
}

function formatSize(bytes: number): string {
  const megabytes = bytes / (1024 * 1024);
  return `${megabytes.toFixed(1)} MB`;
}
