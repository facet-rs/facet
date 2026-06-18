const DEFAULT_SUBJECT_INACTIVITY_TIMEOUT_SECS = 60;

// r[impl hosted.subject.lifecycle]
function subjectInactivityTimeoutMs(): number | null {
  const raw = process.env.SUBJECT_INACTIVITY_TIMEOUT_SECS;
  const secs = raw === undefined ? DEFAULT_SUBJECT_INACTIVITY_TIMEOUT_SECS : Number(raw);
  if (!Number.isFinite(secs) || secs <= 0) {
    return null;
  }
  return secs * 1000;
}

// r[impl hosted.subject.lifecycle]
export async function withSubjectTimeout<T>(mode: string, run: () => Promise<T>): Promise<T> {
  const timeoutMs = subjectInactivityTimeoutMs();
  if (timeoutMs === null) {
    return run();
  }

  let timer: ReturnType<typeof setTimeout> | undefined;
  try {
    return await Promise.race([
      run(),
      new Promise<never>((_, reject) => {
        timer = setTimeout(() => {
          reject(new Error(`subject ${mode} timed out after ${timeoutMs}ms without exiting`));
        }, timeoutMs);
      }),
    ]);
  } finally {
    if (timer !== undefined) {
      clearTimeout(timer);
    }
  }
}
