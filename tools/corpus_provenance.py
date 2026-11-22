"""Observe this checkout's source state independently of the pinned renderer binary."""
import hashlib
import json
import os
from pathlib import Path
import subprocess


def source_snapshot(repository=None):
    root = Path(repository) if repository is not None else Path(__file__).resolve().parent.parent
    listing = subprocess.run(['git', 'ls-files', '--cached', '--others', '--exclude-standard', '-z'],
                             cwd=root, check=True, capture_output=True).stdout
    files = {}
    for name in sorted(set(os.fsdecode(listing).split('\0')) - {''}):
        path = root / name
        if path.is_symlink():
            data = b'symlink\0' + os.fsencode(os.readlink(path))
        elif path.is_file():
            data = b'file\0' + path.read_bytes()
        else:
            files[name] = None
            continue
        files[name] = hashlib.sha256(data).hexdigest()
    revision = subprocess.run(['git', 'rev-parse', '--verify', 'HEAD'], cwd=root,
                              check=False, capture_output=True, text=True)
    return {'revision': revision.stdout.strip() if revision.returncode == 0 else None,
            'sha256': hashlib.sha256(json.dumps(files, sort_keys=True).encode()).hexdigest(), 'files': files}


def compare_snapshots(start, end):
    changed = sorted(name for name in start['files'].keys() | end['files'].keys()
                     if (name in start['files']) != (name in end['files']) or start['files'].get(name) != end['files'].get(name))
    def summary(snapshot):
        return {'revision': snapshot['revision'], 'sha256': snapshot['sha256'], 'file_count': len(snapshot['files'])}
    return {'scope': 'This checkout, including untracked files; ignored output, sibling dependencies and installed fonts excluded.',
            'meaning': 'Observed source state during comparison; not proof of the source used to build the pinned renderer.',
            'start': summary(start), 'end': summary(end), 'changed_files': changed,
            'unchanged': start['sha256'] == end['sha256'] and start['revision'] == end['revision']}
