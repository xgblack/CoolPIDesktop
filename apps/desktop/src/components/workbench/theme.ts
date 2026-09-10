import {useEffect,useState} from 'react';
export type Theme='light'|'dark'|'system';
export function useTheme(){
 const [theme,setTheme]=useState<Theme>(()=>{try{const v=localStorage.getItem('omp-ui-theme');return v==='light'||v==='dark'?v:'system';}catch{return 'system';}});
 const [error,setError]=useState('');
 useEffect(()=>{const media=matchMedia('(prefers-color-scheme: dark)');const apply=()=>{document.documentElement.classList.toggle('dark',theme==='dark'||theme==='system'&&media.matches);};apply();media.addEventListener('change',apply);return()=>media.removeEventListener('change',apply);},[theme]);
 const change=(next:Theme)=>{setTheme(next);try{localStorage.setItem('omp-ui-theme',next);setError('');}catch{setError('主题已应用，但无法保存偏好。');}};
 return{theme,change,error};
}
