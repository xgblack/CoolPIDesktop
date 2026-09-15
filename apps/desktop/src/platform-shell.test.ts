// @vitest-environment jsdom
import {beforeEach,describe,expect,it,vi} from 'vitest';
import {invoke} from '@tauri-apps/api/core';
import {desktopPlatformShell} from './platform-shell';

vi.mock('@tauri-apps/api/core',()=>({invoke:vi.fn().mockResolvedValue(undefined)}));

describe('desktop platform shell',()=>{
  beforeEach(()=>vi.clearAllMocks());

  it('delegates validated external links to the Host',async()=>{
    await desktopPlatformShell.openExternal('https://example.com/docs');
    expect(invoke).toHaveBeenCalledWith('open_external_link',{url:'https://example.com/docs'});
  });

  it('reports unavailable clipboard instead of silently succeeding',async()=>{
    Object.defineProperty(navigator,'clipboard',{configurable:true,value:undefined});
    await expect(desktopPlatformShell.writeClipboard('text')).rejects.toThrow('clipboard_unavailable');
  });
});
