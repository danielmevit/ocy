import fs from 'node:fs';
import path from 'node:path';

export interface ExerciseManifestEntry {
  frames: number;
  sequence: number[];
  stepMs: number;
  frozen: number;
}

export interface ExerciseManifest {
  version: string;
  palette: string;
  viewBox: [number, number, number, number];
  exercises: Record<string, ExerciseManifestEntry>;
}

export interface ExerciseItem {
  id: string;
  title: string;
  body: string;
  frames: number;
  sequence: number[];
  stepMs: number;
  frozen: number;
}

function repoRoot(): string {
  const cwd = process.cwd();
  if (fs.existsSync(path.join(cwd, 'Directory.Build.props'))) {
    return cwd;
  }
  const fromParent = path.resolve(cwd, '..');
  if (fs.existsSync(path.join(fromParent, 'Directory.Build.props'))) {
    return fromParent;
  }
  throw new Error(`Could not locate repository root (Directory.Build.props) from ${cwd}`);
}

export function readAppVersion(): string {
  const propsPath = path.join(repoRoot(), 'Directory.Build.props');
  if (!fs.existsSync(propsPath)) {
    throw new Error(`Directory.Build.props not found at ${propsPath}`);
  }
  const xml = fs.readFileSync(propsPath, 'utf8');
  const match = xml.match(/<Version>\s*([^<\s]+)\s*<\/Version>/);
  if (!match || !match[1]) {
    throw new Error('Could not read <Version> from Directory.Build.props');
  }
  return match[1];
}

export function readExercises(): ExerciseItem[] {
  const root = repoRoot();
  const manifestPath = path.join(root, 'assets/illustrations/ocy-v1f/manifest.json');
  const stringsPath = path.join(root, 'content/strings/en.json');

  const manifest = JSON.parse(fs.readFileSync(manifestPath, 'utf8')) as ExerciseManifest;
  const strings = JSON.parse(fs.readFileSync(stringsPath, 'utf8')) as Record<string, string>;

  return Object.entries(manifest.exercises).map(([id, entry]) => {
    const title = strings[`exercise.${id}.title`];
    const body = strings[`exercise.${id}.body`];
    if (!title || !body) {
      throw new Error(`Missing English strings for exercise "${id}" in content/strings/en.json`);
    }
    return {
      id,
      title,
      body,
      frames: entry.frames,
      sequence: entry.sequence,
      stepMs: entry.stepMs,
      frozen: entry.frozen,
    };
  });
}
