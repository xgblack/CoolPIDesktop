// @vitest-environment jsdom
import {afterEach,expect,it} from 'vitest';
import {act,cleanup,renderHook} from '@testing-library/react';
import {usePreferences} from './preferences';

afterEach(()=>{cleanup();localStorage.clear();document.documentElement.style.removeProperty('--ui-chat-width');document.documentElement.style.removeProperty('--ui-content-font-size');delete document.documentElement.dataset.codeTheme});

it('restores valid preferences and applies reading dimensions',()=>{
 localStorage.setItem('cool-pi-desktop.ui-preferences',JSON.stringify({chatWidth:'wide',fontSize:'large',codeTheme:'monokai',thinkingExpanded:true,notifications:false,sound:false}));
 const {result}=renderHook(usePreferences);
 expect(result.current.preferences.thinkingExpanded).toBe(true);
 expect(document.documentElement.style.getPropertyValue('--ui-chat-width')).toBe('960px');
 expect(document.documentElement.style.getPropertyValue('--ui-content-font-size')).toBe('15px');
 expect(document.documentElement.dataset.codeTheme).toBe('monokai');
 act(()=>result.current.change({chatWidth:'compact',sound:true}));
 expect(document.documentElement.style.getPropertyValue('--ui-chat-width')).toBe('680px');
 expect(JSON.parse(localStorage.getItem('cool-pi-desktop.ui-preferences')!).sound).toBe(true);
});

it('rejects malformed values without losing valid defaults',()=>{
 localStorage.setItem('cool-pi-desktop.ui-preferences',JSON.stringify({chatWidth:'huge',fontSize:'tiny',codeTheme:'remote',thinkingExpanded:'yes'}));
 const {result}=renderHook(usePreferences);
 expect(result.current.preferences).toMatchObject({chatWidth:'standard',fontSize:'standard',codeTheme:'adaptive',thinkingExpanded:false});
});
