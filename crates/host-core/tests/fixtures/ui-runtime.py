#!/usr/bin/env python3
"""Deterministic RPC fixture for extension UI responses; never calls a model."""
import json
import sys


def emit(value):
    print(json.dumps(value), flush=True)


if '--version' in sys.argv:
    print('omp/18.1.15')
    sys.exit(0)

emit({'type': 'ready', 'supportedProtocolVersions': [2], 'maxFrameBytes': 1048576,
      'maxReassembledFrameBytes': 16777216})
for line in sys.stdin:
    command = json.loads(line)
    kind = command['type']
    if kind == 'extension_ui_response':
        emit({'type': 'message_update', 'assistantMessageEvent': {
            'type': 'text_delta', 'delta': json.dumps(command)}})
        emit({'type': 'agent_end', 'isTerminal': True})
        continue
    data = {'negotiate_protocol': {'protocolVersion': 2}, 'get_state': {},
            'get_available_models': {'models': []},
            'get_available_commands': {'commands': []}}.get(kind, {})
    emit({'type': 'response', 'id': command['id'], 'command': kind,
          'success': True, 'data': data})
    if kind == 'prompt':
        emit({'type': 'extension_ui_request', 'id': 'approval-1',
              'method': 'confirm', 'title': 'Fixture confirmation'})
