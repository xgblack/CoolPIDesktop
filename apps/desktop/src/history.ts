import type {HistoryPage,Message} from '../../../packages/host-contract/src';

/** A paging walk belongs to one task/run; only its latest request may publish. */
export class HistoryFeed {
  private revision=0;
  private pending=false;
  messages:Message[]=[];
  cursor:string|null=null;
  reset(){this.revision++;this.pending=false;this.messages=[];this.cursor=null;}
  invalidate(){this.revision++;this.pending=false;}
  async load(fetch:(cursor:string|null)=>Promise<HistoryPage>,more:boolean,publish:()=>void){
    if(more&&(this.pending||!this.cursor))return;
    const revision=++this.revision;
    const cursor=more?this.cursor:null;
    this.pending=true;
    try{
      const page=await fetch(cursor);
      if(revision!==this.revision)return;
      this.messages=more?[...this.messages,...page.messages]:page.messages;
      this.cursor=page.nextCursor??null;
      publish();
    }catch(error){
      if(revision!==this.revision)return;
      this.messages=[];this.cursor=null;publish();throw error;
    }finally{if(revision===this.revision)this.pending=false;}
  }
}
