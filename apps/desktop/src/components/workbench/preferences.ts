import {useEffect,useState} from 'react';

export interface UiPreferences {
 chatWidth:'compact'|'standard'|'wide';
 fontSize:'compact'|'standard'|'large';
 codeTheme:'adaptive'|'github'|'monokai';
 thinkingExpanded:boolean;
 notifications:boolean;
 sound:boolean;
}

const KEY='cool-pi-desktop.ui-preferences';
const defaults:UiPreferences={chatWidth:'standard',fontSize:'standard',codeTheme:'adaptive',thinkingExpanded:false,notifications:true,sound:true};

function read():UiPreferences{
 try{
  const value=JSON.parse(localStorage.getItem(KEY)??'{}') as Partial<UiPreferences>;
  return {
   chatWidth:['compact','standard','wide'].includes(value.chatWidth??'')?value.chatWidth!:defaults.chatWidth,
   fontSize:['compact','standard','large'].includes(value.fontSize??'')?value.fontSize!:defaults.fontSize,
   codeTheme:['adaptive','github','monokai'].includes(value.codeTheme??'')?value.codeTheme!:defaults.codeTheme,
   thinkingExpanded:typeof value.thinkingExpanded==='boolean'?value.thinkingExpanded:defaults.thinkingExpanded,
   notifications:typeof value.notifications==='boolean'?value.notifications:defaults.notifications,
   sound:typeof value.sound==='boolean'?value.sound:defaults.sound,
  };
 }catch{return defaults}
}

export function usePreferences(){
 const [preferences,setPreferences]=useState<UiPreferences>(read),[error,setError]=useState('');
 useEffect(()=>{
  const root=document.documentElement;
  root.style.setProperty('--ui-chat-width',preferences.chatWidth==='compact'?'680px':preferences.chatWidth==='wide'?'960px':'800px');
  root.style.setProperty('--ui-content-font-size',preferences.fontSize==='compact'?'13px':preferences.fontSize==='large'?'15px':'14px');
  root.dataset.codeTheme=preferences.codeTheme;
 },[preferences.chatWidth,preferences.fontSize,preferences.codeTheme]);
 const change=(patch:Partial<UiPreferences>)=>{
  setPreferences(current=>{
   const next={...current,...patch};
   try{localStorage.setItem(KEY,JSON.stringify(next));setError('')}catch{setError('偏好已应用，但无法保存。')}
   return next;
  });
 };
 return{preferences,change,error};
}
