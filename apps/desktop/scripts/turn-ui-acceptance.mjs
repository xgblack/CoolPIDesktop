import {chromium} from '@playwright/test';
import {mkdir} from 'node:fs/promises';
import assert from 'node:assert/strict';
const output=process.env.UI_SCREENSHOTS??'/tmp/cool-pi-turn-ui';
await mkdir(output,{recursive:true});
const browser=await chromium.launch({channel:'chrome',headless:true});
try {
 for (const width of [1280,390]) for (const theme of ['light','dark']) {
  const page=await browser.newPage({viewport:{width,height:844},colorScheme:theme});
  const errors=[];
  page.on('pageerror',e=>{errors.push(e.message);console.error(e.message);});
  page.on('console',e=>{if(e.type()==='error')errors.push(e.text());});
  await page.addInitScript(()=>{
   const started=Date.now()-123000;
   const project={id:'p',name:'Cool PI Desktop',roots:['/tmp/turn-ui'],trusted:true,archived:false};
   const tasks=[{id:'a',projectId:'p',title:'对话底部信息验收',roots:project.roots,sessionId:'omp-a',sessionFile:'/tmp/a.jsonl',model:'test/model',pinned:false,archived:false}];
   const usage=(input,output)=>({input,output,cacheRead:0,cacheWrite:0,totalTokens:input+output,cost:{total:0.002}});
   const history={a:[{role:'user',content:'参考 DSH 和 Codex，为每轮回答增加操作和统计信息。',timestamp:started},{role:'assistant',timestamp:started+200,completedAt:started+1000,content:[{type:'thinking',thinking:'检查 OMP 会话边界与用量来源。'},{type:'toolCall',id:'read-1',name:'read',arguments:{path:'message-list.tsx'}}],usage:usage(800,75)},{role:'toolResult',toolCallId:'read-1',content:'已找到对话组件',timestamp:started+1100},{role:'assistant',timestamp:started+1200,completedAt:started+65000,content:[{type:'text',text:'已为每轮回答补齐底部信息。\n\n- **复制**：保留回答原始 Markdown。\n- **用量**：汇总整轮模型调用，可展开明细。\n- **Fork**：从这一轮创建独立会话，原对话保持不变。\n\n执行过程中，计时会持续更新；切换任务后仍显示真实的执行时间。'}],usage:usage(300,100)}]};
   const runs={a:{taskId:'a',runId:'a-run',seq:1,status:'ready',text:'',events:[],pendingUi:[],tools:[],runtime:{status:'ready',capabilities:{models:[{provider:'test',id:'model'}],state:{model:{provider:'test',id:'model'}}}}}};
   let socket;
   const publish=()=>socket?.onmessage?.({data:JSON.stringify({type:'snapshot',tasks:Object.values(runs)})});
   window.WebSocket=class{constructor(){socket=this;setTimeout(()=>this.onopen?.(),0);}send(){setTimeout(publish,0);}close(){}};
   Object.defineProperty(navigator,'clipboard',{configurable:true,value:{writeText:async text=>{window.__copied=text;}}});
   window.__complete=()=>{const run=runs.a;history.a.push({role:'assistant',timestamp:run.turnStartedAt+1,completedAt:Date.now(),content:'计时验收完成。',usage:usage(10,5)});run.status='idle';run.turnCompletedAt=Date.now();run.seq++;publish();};
   window.__TAURI_INTERNALS__={metadata:{currentWindow:{label:'main'},currentWebview:{label:'main'}},transformCallback:()=>1,invoke:async(cmd,args={})=>{
    switch(cmd){
     case 'plugin:event|listen':return 1;case 'plugin:event|unlisten':return null;
     case 'list_projects':return[project];case 'task_records':return structuredClone(tasks);case 'list_tasks':return structuredClone(Object.values(runs));
     case 'observer_info':return{url:'ws://127.0.0.1:9999',token:'fixture'};
     case 'task_snapshot':case 'continue_task':return structuredClone(runs[args.taskId]);
     case 'task_history':return{messages:structuredClone(history[args.taskId]??[]),totalMessages:history[args.taskId]?.length??0,nextCursor:null};
     case 'model_config_verify':return{defaultModel:'test/model',models:[{provider:'test',id:'model'}],stage:'loaded',message:'loaded'};
     case 'runtime_tasks':return[];case 'runtime_focus':return null;
     case 'task_attachments':case 'task_roots':return[];
     case 'list_task_files':return{entries:[],truncated:false};
     case 'prompt_task':{const run=runs[args.taskId];run.turnStartedAt=Date.now();run.turnCompletedAt=null;run.status='running';run.seq++;run.tools=[{id:'live',name:'read',args:{path:'src/workbench.tsx'},status:'running',seq:run.seq}];history.a.push({role:'user',content:args.message,timestamp:run.turnStartedAt});publish();return structuredClone(run);}
     case 'fork_task':{window.__forkTimestamp=args.timestamp;const fork={...tasks[0],id:'fork',title:'对话底部信息验收 · Fork',sessionId:'omp-fork',sessionFile:'/tmp/fork.jsonl'};tasks.push(fork);history.fork=structuredClone(history.a.slice(0,4));runs.fork={...runs.a,taskId:'fork',runId:'fork-run',seq:1,status:'ready'};return fork;}
     default:throw new Error('Unexpected fixture command '+cmd);
    }
   }};
  });
  await page.goto('http://127.0.0.1:5173/');
  if(width<1024)await page.getByRole('button',{name:'打开侧栏',exact:true}).click();
  await page.getByRole('button',{name:'Cool PI Desktop',exact:true}).click({timeout:6000}).catch(async e=>{console.error(await page.locator('body').innerText());throw e;});
  await page.getByRole('button',{name:'对话底部信息验收',exact:true}).click();
  await page.getByText('1,275 tokens',{exact:true}).waitFor();
  assert.equal(await page.getByText('用时 1分5秒',{exact:true}).count(),1);
  await page.getByRole('button',{name:'复制回答',exact:true}).click();
  assert((await page.evaluate(()=>window.__copied)).includes('**复制**'));
  await page.screenshot({path:`${output}/${width}-${theme}-footer.png`});
  await page.getByRole('button',{name:'查看本轮用量'}).click();
  await page.getByRole('dialog',{name:'本轮用量'}).waitFor();
  await page.getByText('1,100',{exact:true}).waitFor();
  await page.screenshot({path:`${output}/${width}-${theme}-usage.png`});
  await page.keyboard.press('Escape');
  const input=page.getByRole('textbox',{name:'消息',exact:true});
  await input.fill('验证实时计时');await input.press('Enter');
  await page.getByText('正在执行 · 0秒',{exact:true}).waitFor();
  await page.getByText('正在执行 · 2秒',{exact:true}).waitFor();
  assert(await page.getByRole('button',{name:'从此处 Fork'}).isDisabled());
  await page.screenshot({path:`${output}/${width}-${theme}-running.png`});
  await page.evaluate(()=>window.__complete());
  await page.getByText('计时验收完成。',{exact:true}).waitFor();
  assert.equal(await page.getByText(/正在执行 ·/).count(),0);
  await page.getByRole('button',{name:'从此处 Fork'}).first().click();
  await page.getByRole('heading',{name:'对话底部信息验收 · Fork',exact:true}).waitFor();
  assert.equal(await page.getByText('计时验收完成。',{exact:true}).count(),0);
  assert.equal(await page.getByText('1,275 tokens',{exact:true}).count(),1);
  assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth>innerWidth),false);
  assert.deepEqual(errors,[]);
  await page.close();
 }
 console.log('PASS turn UI: four viewport/theme combinations, copy, usage details, live clock, fork navigation and no overflow/console errors; '+output);
} finally {await browser.close();}
