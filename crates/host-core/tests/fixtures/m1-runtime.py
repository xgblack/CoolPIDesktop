#!/usr/bin/env python3
"""Deterministic OMP 18.1.20 RPC control fixture; never calls a model."""
import json
import sys
import uuid


def emit(value):
    print(json.dumps(value), flush=True)


if '--version' in sys.argv:
    print('omp/18.1.20')
    sys.exit(0)

session_id = str(uuid.uuid4())
streaming = False
queued = 0
commands = [
    {'name': 'review', 'aliases': ['r'], 'description': 'Review changes', 'source': 'extension'},
]
subagent = {'id': 'sub-1', 'index': 1, 'agent': 'worker', 'status': 'running', 'lastUpdate': 1}
state = {
    'sessionId': session_id,
    'isStreaming': False,
    'isCompacting': False,
    'queuedMessageCount': 0,
    'model': {'provider': 'test', 'id': 'model'},
    'thinkingLevel': 'medium',
}

emit({'type': 'ready', 'protocolVersion': 1, 'supportedProtocolVersions': [1, 2],
      'maxFrameBytes': 1048576, 'maxReassembledFrameBytes': 16777216})
for line in sys.stdin:
    command = json.loads(line)
    kind = command['type']
    ident = command.get('id')
    if kind == 'negotiate_protocol':
        data = {'protocolVersion': 2}
    elif kind == 'get_state':
        state.update(isStreaming=streaming, queuedMessageCount=queued)
        data = state
    elif kind == 'get_available_models':
        data = {'models': [{'provider': 'test', 'id': 'model', 'reasoning': True}]}
    elif kind == 'get_available_commands':
        data = {'commands': commands}
    elif kind == 'get_login_providers':
        data = {'providers': [{'id': 'oauth-test', 'name': 'OAuth Test',
                               'available': True, 'authenticated': False}]}
    elif kind == 'login':
        if command.get('providerId') != 'oauth-test':
            emit({'type': 'response', 'id': ident, 'command': kind, 'success': False,
                  'code': 'unknown_provider', 'error': 'unknown provider'})
            continue
        data = {'providerId': command['providerId']}
    elif kind == 'set_subagent_subscription':
        data = {'level': command['level']}
    elif kind == 'get_subagents':
        data = {'subagents': [subagent]}
    elif kind == 'get_subagent_messages':
        data = {'sessionFile': '/fixture/sub.jsonl', 'fromByte': command.get('fromByte', 0),
                'nextByte': 42, 'reset': False, 'entries': [],
                'messages': [{'role': 'assistant', 'content': 'subagent output'}]}
    elif kind == 'set_thinking_level':
        if command['level'] == 'max':
            emit({'type': 'response', 'id': ident, 'command': kind, 'success': False,
                  'code': 'unsupported_thinking_level', 'error': 'max is unavailable'})
            continue
        state['thinkingLevel'] = command['level']
        data = None
    elif kind in ('set_steering_mode', 'set_follow_up_mode', 'set_interrupt_mode',
                  'set_auto_compaction', 'set_auto_retry', 'abort_retry'):
        data = None
    elif kind == 'compact':
        emit({'type': 'auto_compaction_start', 'reason': 'manual'})
        emit({'type': 'response', 'id': ident, 'command': kind, 'success': True,
              'data': {'summary': 'compact'}})
        emit({'type': 'auto_compaction_end', 'aborted': False})
        continue
    elif kind == 'prompt':
        if command['message'] == 'exit':
            sys.exit(7)
        streaming = True
        emit({'type': 'response', 'id': ident, 'command': kind, 'success': True,
              'data': {'agentInvoked': True}})
        emit({'type': 'agent_start'})
        if command['message'] == 'ui':
            emit({'type': 'extension_ui_request', 'id': 'select-1', 'method': 'select',
                  'title': 'Choose', 'options': ['one', 'two'],
                  'optionDetails': [{'description': 'first'}, {'description': 'second'}]})
            emit({'type': 'extension_ui_request', 'id': 'notify-1', 'method': 'notify',
                  'message': 'notice', 'notifyType': 'info'})
            emit({'type': 'extension_ui_request', 'id': 'status-1', 'method': 'setStatus',
                  'statusKey': 'mode', 'statusText': 'testing'})
            emit({'type': 'extension_ui_request', 'id': 'widget-1', 'method': 'setWidget',
                  'widgetKey': 'summary', 'widgetLines': ['line'], 'widgetPlacement': 'aboveEditor'})
            emit({'type': 'extension_ui_request', 'id': 'title-1', 'method': 'setTitle', 'title': 'Fixture'})
            emit({'type': 'extension_ui_request', 'id': 'editor-1', 'method': 'set_editor_text', 'text': 'draft'})
            emit({'type': 'extension_ui_request', 'id': 'url-1', 'method': 'open_url',
                  'url': 'https://example.com/full', 'launchUrl': 'http://127.0.0.1/link'})
        continue
    elif kind in ('steer', 'follow_up'):
        queued += 2 if command['message'] == 'desync' else 1
        data = None
    elif kind == 'abort_and_prompt':
        streaming = True
        data = None
    elif kind == 'abort':
        streaming = False
        queued = 0
        emit({'type': 'response', 'id': ident, 'command': kind, 'success': True})
        emit({'type': 'agent_end', 'isTerminal': True})
        continue
    elif kind == 'extension_ui_response':
        data = None
    else:
        emit({'type': 'response', 'id': ident, 'command': kind, 'success': False,
              'code': 'unsupported', 'error': 'unsupported command'})
        continue
    emit({'type': 'response', 'id': ident, 'command': kind, 'success': True, 'data': data})
