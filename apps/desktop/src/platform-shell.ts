import {invoke} from '@tauri-apps/api/core';
import type {CapabilitySupport} from '../../../packages/host-contract/src';

export interface PlatformShellCapabilities {
  clipboard:CapabilitySupport;
  externalLinks:CapabilitySupport;
  notifications:CapabilitySupport;
  saveDialog:CapabilitySupport;
  updater:CapabilitySupport;
  windowManagement:CapabilitySupport;
}

export interface PlatformShell {
  kind:'desktop'|'browser';
  capabilities:PlatformShellCapabilities;
  writeClipboard(text:string):Promise<void>;
  openExternal(url:string):Promise<void>;
  notify(title:string,options?:NotificationOptions):Promise<boolean>;
  playCompletionSound():Promise<boolean>;
}

function notificationSupport():CapabilitySupport {
  return typeof Notification==='undefined'?'unsupported':'supported';
}

export const desktopPlatformShell:PlatformShell={
  kind:'desktop',
  capabilities:{
    clipboard:'supported',
    externalLinks:'supported',
    notifications:notificationSupport(),
    saveDialog:'supported',
    updater:'unsupported',
    windowManagement:'supported',
  },
  async writeClipboard(text){
    if(!navigator.clipboard?.writeText)throw new Error('clipboard_unavailable');
    await navigator.clipboard.writeText(text);
  },
  async openExternal(url){await invoke('open_external_link',{url});},
  async notify(title,options){
    if(typeof Notification==='undefined'||Notification.permission!=='granted')return false;
    new Notification(title,options);
    return true;
  },
  async playCompletionSound(){
    const AudioContextClass=window.AudioContext;
    if(!AudioContextClass)return false;
    const context=new AudioContextClass();
    const oscillator=context.createOscillator(),gain=context.createGain();
    oscillator.type='sine';oscillator.frequency.setValueAtTime(660,context.currentTime);gain.gain.setValueAtTime(.045,context.currentTime);gain.gain.exponentialRampToValueAtTime(.001,context.currentTime+.16);
    oscillator.connect(gain);gain.connect(context.destination);oscillator.start();oscillator.stop(context.currentTime+.16);oscillator.addEventListener('ended',()=>void context.close(),{once:true});
    return true;
  },
};
