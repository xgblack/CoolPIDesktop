#!/usr/bin/env python3
"""Model-specific capability and launch-argument fixture; never contacts a provider."""
import json
import pathlib
import sys
import uuid

models = [dict(provider='test', id='wide', reasoning=True, thinking=['low', 'high']),
          dict(provider='test', id='narrow', reasoning=True, thinking=['low']),
          dict(provider='test', id='fixed', reasoning=True, thinking=[]),
          dict(provider='test', id='unknown', reasoning=True)]
def emit(value):
    print(json.dumps(value), flush=True)
def arg(name, default=None):
    return sys.argv[sys.argv.index(name) + 1] if name in sys.argv else default
if '--version' in sys.argv:
    print('omp/18.1.20')
    sys.exit(0)
if sys.argv[1] == 'models':
    emit(dict(models=models))
    sys.exit(0)
if sys.argv[1] == 'config':
    emit(dict(value=dict(default='test/wide')))
    sys.exit(0)
# Real RPC returns objects even though the CLI models command returns arrays.
rpc_models = [{**m, 'thinking': {'mode': 'effort', 'efforts': m['thinking']}}
              if 'thinking' in m else m for m in models]
root = pathlib.Path(arg('--cwd'))
file = pathlib.Path(arg('--session', str(pathlib.Path(arg('--session-dir')) / 'session.jsonl')))
session_id = json.loads(file.read_text().splitlines()[0])['id'] if file.exists() else str(uuid.uuid4())
if not file.exists():
    file.write_text(json.dumps(dict(type='session', version=3, id=session_id, cwd=str(root))) + '\n')
selected = arg('--model', 'test/wide').split('/')[-1]
level = arg('--thinking', 'low')
emit(dict(type='ready', supportedProtocolVersions=[2], maxFrameBytes=1048576, maxReassembledFrameBytes=16777216))
for line in sys.stdin:
    c = json.loads(line)
    kind = c['type']
    if kind == 'set_model':
        selected = c['modelId']
    state = dict(sessionId=session_id, sessionFile=str(file), isStreaming=False, isCompacting=False,
                 queuedMessageCount=0, model=dict(provider='test', id=selected), thinkingLevel=level, fixtureArgs=sys.argv[1:])
    data = {'get_state': state, 'negotiate_protocol': dict(protocolVersion=2),
            'get_available_models': dict(models=rpc_models), 'get_available_commands': dict(commands=[])}.get(kind, {})
    emit(dict(type='response', id=c.get('id'), command=kind, success=True, data=data))
