#!/usr/bin/env python3
"""Import existing MSGA Slack sessions directly into Chord's local bridge."""
import argparse
import configparser
import json
from pathlib import Path
import re
import urllib.error
import urllib.parse
import urllib.request


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--workspace', help='Import only this Slack team ID')
    args = parser.parse_args()
    root = Path(__file__).resolve().parent.parent
    config = json.loads((root / 'private/config.json').read_text())
    settings = configparser.RawConfigParser(interpolation=None, strict=False)
    settings.optionxform = str
    settings.read(Path.home() / '.config/msga/msga.conf')

    def local(action, body):
        request = urllib.request.Request(
            'http://127.0.0.1:4789/api/bridges/slack/' + action,
            data=json.dumps(body).encode(),
            headers={'Authorization': 'Bearer ' + config['apiKey'],
                     'Content-Type': 'application/json'},
        )
        try:
            return json.load(urllib.request.urlopen(request, timeout=90))
        except urllib.error.HTTPError as error:
            # Bridge responses are not printed: auth values may appear in errors.
            raise RuntimeError(f'Local bridge rejected login, HTTP {error.code}') from None

    failed = False
    for key, value in settings.items('workspace'):
        match = re.fullmatch(r'slack%3A([A-Z0-9]+)\\auth', key)
        if not match or args.workspace and args.workspace != match[1]:
            continue
        team = match[1]
        try:
            # QSettings stores a quoted JSON string. Its d cookie is URL encoded.
            auth = json.loads(value)
            if isinstance(auth, str):
                auth = json.loads(auth)
            token = auth['xoxp']
            cookie = urllib.parse.unquote(auth['cookie'])
            if not token.startswith('xoxc-') or not re.fullmatch(r'xoxd-[A-Za-z0-9/+=]+', cookie):
                raise RuntimeError('Unsupported MSGA session format')
            request = urllib.request.Request(
                'https://slack.com/api/auth.test',
                data=urllib.parse.urlencode({'token': token}).encode(),
                headers={'Cookie': 'd=' + urllib.parse.quote(cookie, safe='')},
            )
            identity = json.load(urllib.request.urlopen(request, timeout=30))
            if not identity.get('ok') or identity.get('team_id') != team:
                raise RuntimeError('MSGA session needs renewal or has the wrong workspace')
            local('start', {'flow': 'token'})
            result = local('submit', {'auth_token': token, 'cookie_token': cookie})
            if result.get('type') != 'complete':
                raise RuntimeError('Slack login did not complete')
            print(f'{team}: connected')
        except Exception as error:
            failed = True
            print(f'{team}: {type(error).__name__}: {error}' if isinstance(error, RuntimeError)
                  else f'{team}: import failed, {type(error).__name__}')
    raise SystemExit(1 if failed else 0)


if __name__ == '__main__':
    main()
