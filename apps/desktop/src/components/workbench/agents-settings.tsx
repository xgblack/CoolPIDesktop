import {useEffect,useState} from 'react';
import {FilePlus2,LoaderCircle,RefreshCw,Save} from 'lucide-react';
import type {AgentProfile,HostError,Project} from '../../../../../packages/host-contract/src';
import {agentProfiles,hostError} from '@/host';
import {Button} from '@/components/ui/button';
import {Input} from '@/components/ui/input';
import {Textarea} from '@/components/ui/textarea';
import {ErrorNotice} from './shared';

const TEMPLATE='---\nname: new-agent\ndescription: Describe when to use this agent.\n---\n\nYou are a focused OMP agent.\n';

export function AgentsSettings({projects,projectId,onDirtyChange}:{projects:Project[];projectId:string;onDirtyChange:(value:boolean)=>void}){
 const [scope,setScope]=useState<'user'|'project'>('user'),[selectedProject,setSelectedProject]=useState(projectId),[profiles,setProfiles]=useState<AgentProfile[]>([]),[selected,setSelected]=useState(''),[name,setName]=useState(''),[content,setContent]=useState(''),[revision,setRevision]=useState(''),[baseline,setBaseline]=useState(''),[busy,setBusy]=useState(false),[error,setError]=useState<HostError|null>(null);
 const dirty=content!==baseline||(!revision&&!!name);
 useEffect(()=>onDirtyChange(dirty),[dirty,onDirtyChange]);
 const load=async()=>{if(scope==='project'&&!selectedProject){setProfiles([]);return}setBusy(true);setError(null);try{const result=await agentProfiles.list(scope,scope==='project'?selectedProject:null,0);setProfiles(result);const current=result.find(item=>item.name===selected)??result[0];if(current){setSelected(current.name);setName(current.name);setContent(current.content);setBaseline(current.content);setRevision(current.revision)}else{setSelected('');setName('');setContent('');setBaseline('');setRevision('')}}catch(value){setError(hostError(value));}finally{setBusy(false)}};
 useEffect(()=>{void load();},[scope,selectedProject]);
 const choose=(profile:AgentProfile)=>{if(dirty&&!confirm('放弃当前未保存的 Profile 修改？'))return;setSelected(profile.name);setName(profile.name);setContent(profile.content);setBaseline(profile.content);setRevision(profile.revision);setError(null)};
 const create=()=>{if(dirty&&!confirm('放弃当前未保存的 Profile 修改？'))return;setSelected('');setName('new-agent');setContent(TEMPLATE);setBaseline('');setRevision('');setError(null)};
 const save=async()=>{setBusy(true);setError(null);try{const result=await agentProfiles.save(scope,scope==='project'?selectedProject:null,scope==='project'?0:null,name,content,revision);setProfiles(old=>[...old.filter(item=>item.name!==result.name),result].sort((a,b)=>a.name.localeCompare(b.name)));setSelected(result.name);setName(result.name);setContent(result.content);setBaseline(result.content);setRevision(result.revision)}catch(value){setError(hostError(value));}finally{setBusy(false)}};
 return <div className="agent-settings-layout">
  <aside><div className="agent-scope" role="group" aria-label="Agent Profile 范围"><Button variant={scope==='user'?'secondary':'ghost'} aria-pressed={scope==='user'} onClick={()=>setScope('user')}>用户级</Button><Button variant={scope==='project'?'secondary':'ghost'} aria-pressed={scope==='project'} onClick={()=>setScope('project')}>项目级</Button></div>{scope==='project'&&<select aria-label="Profile 项目" value={selectedProject} onChange={event=>setSelectedProject(event.target.value)}>{!projects.length&&<option value="">没有项目</option>}{projects.map(project=><option key={project.id} value={project.id}>{project.name}</option>)}</select>}<div className="agent-profile-list">{profiles.map(profile=><button key={profile.name} aria-current={selected===profile.name?'page':undefined} onClick={()=>choose(profile)}>{profile.name}</button>)}{!busy&&!profiles.length&&<p>此范围暂无 Profile</p>}</div><div className="agent-list-actions"><Button variant="ghost" size="sm" onClick={create}><FilePlus2/>新建</Button><Button variant="ghost" size="icon-sm" aria-label="刷新 Profile" disabled={busy} onClick={()=>void load()}><RefreshCw/></Button></div></aside>
  <section><label>文件名<Input value={name} disabled={!!revision||busy} maxLength={80} onChange={event=>setName(event.target.value)} placeholder="reviewer"/></label><label>Markdown<Textarea className="agent-profile-editor" value={content} disabled={busy} onChange={event=>setContent(event.target.value)} spellCheck={false}/></label><div className="agent-profile-actions"><span>{scope==='user'?'~/.omp/agent/agents':'项目 .omp/agents'} · 未知 frontmatter 原样保留</span><Button disabled={busy||!dirty||!name||!content} onClick={()=>void save()}>{busy?<LoaderCircle className="animate-spin"/>:<Save/>}保存</Button></div><ErrorNotice error={error}/></section>
 </div>;
}
