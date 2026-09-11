import {chromium} from '@playwright/test';
import {mkdir} from 'node:fs/promises';
import assert from 'node:assert/strict';

const output=process.env.UI_SCREENSHOTS??'/tmp/cool-pi-ui-acceptance';
await mkdir(output,{recursive:true});
const browser=await chromium.launch({channel:'chrome',headless:true});
try {
 for(const [width,height] of [[1280,800],[1024,700],[390,844]])for(const theme of ['light','dark']){
  const page=await browser.newPage({viewport:{width,height},colorScheme:theme});
  const errors=[];page.on('pageerror',e=>errors.push(e.message));
  await page.addInitScript(()=>{
   const project={id:'p',name:'长中文项目 · OMP 工作台界面改造与回归验证',roots:['/Users/developer/workspaces/长路径项目/客户端工作台与模型配置预置上下文目录','/tmp/additional-root'],archived:false,trusted:true};
   const task=id=>({id,projectId:'p',title:id==='a'?'审查工作台界面：长中文标题、代码与工具执行输出':'第二个任务 · 草稿和审批隔离',roots:project.roots,pinned:id==='a',archived:false,sessionId:id,sessionFile:'/tmp/'+id,model:'test/qwen3.7-flash'});
   const runs={};
   const snapshot=id=>({taskId:id,runId:id+'-run',seq:1,status:'ready',events:[],text:'',pendingUi:[],runtime:{status:'ready',capabilities:{models:[{provider:'test',id:'qwen3.7-flash'},{provider:'test',id:'another-model'}],state:{model:{provider:'test',id:'qwen3.7-flash'}}}}});
   const messages=[{role:'user',content:'请评估这个工作台的任务隔离与消息呈现，并给出具体修改建议。',timestamp:1},{role:'assistant',timestamp:2,content:[{type:'thinking',thinking:'检查异步响应与工作台布局。'},{type:'text',text:'## 工作台验证\n\n同一项目下，每个任务保留独立的 **会话、草稿和模型**。\n\n- 按需加载会话，不自动重发消息\n- 切换任务后旧审批失效\n\n```typescript\nconst description = "这是一段很长的代码，不应撑破界面";\nconst path = "/a/very/long/path/that/should/remain/in/the/code/scroll/container/without/expanding/the/workbench";\nawait continueTask(task.id);\n```\n\n[官方文档](https://example.com) · ![远程图片](https://example.com/no-fetch.png)\n\n| 验证项 | 结果 |\n| --- | --- |\n| 草稿隔离 | 保留当前输入 |\n| 异步响应 | 丢弃过期结果 |'},{type:'toolCall',name:'read',arguments:{path:'src/workbench.tsx'}}]},{role:'toolResult',toolCallId:'tool1',timestamp:3,content:'检查完成：任务 A 与任务 B 使用独立会话。\n'.repeat(3)}];
   window.__calls=[];
   window.__TAURI_INTERNALS__={invoke:async(cmd,args={})=>{window.__calls.push({cmd,args});switch(cmd){
    case 'list_projects':return[project];case 'task_records':return[task('a'),task('b')];case 'list_tasks':return Object.values(runs);
    case 'continue_task':runs[args.taskId]=snapshot(args.taskId);return runs[args.taskId];
    case 'task_git_status':return{rootIndex:args.rootIndex,available:true,branch:'main',changes:[{path:'src/workbench.tsx',indexStatus:'',worktreeStatus:'M',kind:'modified'}]};
    case 'task_git_diff':return{rootIndex:args.rootIndex,path:args.path,staged:false,binary:false,text:'diff --git a/src/workbench.tsx b/src/workbench.tsx\n-old content\n+new content'};
    case 'task_usage':return runs[args.taskId];
    case 'task_history':return{messages:args.taskId==='a'?messages:[{role:'assistant',content:'任务 B 的独立消息',timestamp:4}],totalMessages:3};
    case 'select_task_model':throw{code:'model_unavailable',message:'模型切换失败，保留当前实际模型',suggestion:'检查模型配置。'};
    case 'model_config_load':return{path:'/isolated/models.yml',exists:false,revision:'missing',providers:[]};
    case 'runtime_status':return{status:'model_required',version:'18.1.15',error:{code:'model_required',message:'没有可用模型',suggestion:'请先配置 OMP 模型。'}};
    default:throw{code:'fixture_unsupported',message:cmd};
   }}};
  });
  await page.goto(process.env.UI_BASE_URL??'http://127.0.0.1:5173');
  const openSidebar=async()=>{if(width<900)await page.getByRole('button',{name:'打开侧栏',exact:true}).click();};
  await openSidebar();
  await page.getByRole('button',{name:'选择项目',exact:true}).click();
  await page.getByRole('menuitem',{name:'长中文项目',exact:false}).click();
  await page.getByRole('button',{name:/审查工作台界面：.*未运行/}).click();
  assert.equal(await page.evaluate(()=>window.__calls.some(x=>x.cmd==='continue_task')),false);
  await page.getByRole('button',{name:'继续',exact:true}).click();
  await page.getByText('同一项目下，每个任务保留独立的',{exact:false}).waitFor();
  await page.getByRole('textbox',{name:'消息',exact:true}).fill('任务 A 草稿 · 不应发送到任务 B');
  await openSidebar();await page.getByRole('button',{name:/第二个任务 · 草稿和审批隔离.*未运行/}).click();
  assert.equal(await page.getByRole('textbox',{name:'消息',exact:true}).inputValue(),'');
  await page.getByRole('textbox',{name:'消息',exact:true}).fill('任务 B 草稿');
  await openSidebar();await page.getByRole('button',{name:/审查工作台界面：.*就绪/}).click();
  assert.equal(await page.getByRole('textbox',{name:'消息',exact:true}).inputValue(),'任务 A 草稿 · 不应发送到任务 B');
  await page.getByRole('region',{name:'对话消息'}).evaluate(e=>{e.scrollTop=0;});
  assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth>innerWidth),false);
  assert.equal(await page.locator('img').count(),0);
  await page.screenshot({path:output+'/'+width+'-'+height+'-'+theme+'.png'});
  if(width<900)await page.getByRole('button',{name:'工具、用量与变更',exact:true}).click();
  await page.getByRole('button',{name:'工作区 M src/workbench.tsx'}).click();
  await page.getByText('+new content',{exact:false}).waitFor();
  await page.screenshot({path:output+'/'+width+'-'+height+'-'+theme+'-inspector.png'});
  if(width<900)await page.keyboard.press('Escape');
  await page.getByRole('combobox',{name:'任务模型'}).click();await page.getByRole('option',{name:'test/another-model'}).click();
  await page.getByText('模型切换失败，保留当前实际模型').waitFor();
  assert.match(await page.getByRole('combobox',{name:'任务模型'}).innerText(),/qwen3.7-flash/);
  await openSidebar();await page.getByRole('button',{name:/设置 本机 OMP/}).click();
  await page.getByRole('button',{name:'外观与运行时',exact:true}).click();
  await page.getByRole('button',{name:'检测 OMP',exact:true}).click();await page.getByText('没有可用模型',{exact:true}).waitFor();
  await page.screenshot({path:output+'/'+width+'-'+height+'-'+theme+'-settings.png'});
  await page.keyboard.press('Escape');
  assert.equal(await page.getByRole('dialog').count(),width<900?1:0);
  assert.deepEqual(errors,[]);
  await page.close();
 }
 console.log('UI acceptance: 6 viewport/theme combinations passed; 18 screenshots in '+output);
}finally{await browser.close();}
