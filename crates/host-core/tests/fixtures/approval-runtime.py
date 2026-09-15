#!/usr/bin/env python3
"""Approval lifecycle fixture; records argv and simulates busy/startup failures."""
import json
import pathlib
import sys
import uuid

if '--version' in sys.argv:
    print('omp/18.1.20')
    sys.exit(0)

def arg(name, default=None):
    return sys.argv[sys.argv.index(name) + 1] if name in sys.argv else default

root = pathlib.Path(arg('--cwd'))
if (root / 'fail-start').exists():
    sys.exit(1)
file = pathlib.Path(arg('--session', str(pathlib.Path(arg('--session-dir')) / 'session.jsonl')))
session_id = json.loads(file.read_text().splitlines()[0])['id'] if file.exists() else str(uuid.uuid4())
if not file.exists() and not (root / 'unsaved').exists():
    file.write_text(json.dumps({'type': 'session', 'version': 3, 'id': session_id, 'cwd': str(root)}) + '\n')

def emit(value):
    print(json.dumps(value), flush=True)

emit({'type': 'ready', 'supportedProtocolVersions': [2], 'maxFrameBytes': 1048576, 'maxReassembledFrameBytes': 16777216})
for line in sys.stdin:
    command = json.loads(line)
    kind = command['type']
    state = {'sessionId': session_id, 'sessionFile': str(file), 'isStreaming': False,
             'isCompacting': False, 'queuedMessageCount': 0, 'model': {'provider': 'test', 'id': 'model'},
             'fixtureApproval': arg('--approval-mode'), 'fixtureArgs': sys.argv[1:]}
    if (root / 'state.json').exists():
        state.update(json.loads((root / 'state.json').read_text()))
    data = {'get_state': state, 'negotiate_protocol': {'protocolVersion': 2},
            'get_available_models': {'models': []}, 'get_available_commands': {'commands': []}}.get(kind, {})
    emit({'type': 'response', 'id': command.get('id'), 'command': kind, 'success': True, 'data': data})
    if kind == 'prompt':
        emit({'type': 'extension_ui_request', 'id': 'pending', 'method': 'confirm', 'title': 'Approval'})
