use host_core::store::Store;
#[tokio::test]
async fn project_edit_preserves_tasks_and_rolls_back_conflicts() {
 let root=std::env::temp_dir().join(format!("project-edit-{}",uuid::Uuid::new_v4()));
 for dir in ["a","b","c"] {std::fs::create_dir_all(root.join(dir)).unwrap();}
 let store=Store::open(root.join("db.sqlite")).await.unwrap();
 let a=root.join("a");let b=root.join("b");let c=root.join("c");
 let p=store.register_project("old",vec![a.clone()],true).await.unwrap();
 let old=store.create_task(&p.id,"old task").await.unwrap();
 store.register_project("other",vec![c.clone()],true).await.unwrap();
 let edited=store.edit_project(&p.id,"new",vec![b.clone(),a.clone()],true).await.unwrap();
 assert_eq!(edited.roots,vec![b.canonicalize().unwrap(),a.canonicalize().unwrap()]);
 assert_eq!(store.task(&old.id).await.unwrap().roots,old.roots);
 assert_eq!(store.create_task(&p.id,"new task").await.unwrap().roots,edited.roots);
 assert!(store.edit_project(&p.id,"bad",vec![c],true).await.is_err());
 let retained=store.projects().await.unwrap().into_iter().find(|x|x.id==p.id).unwrap();
 assert_eq!(retained.name,"new");assert_eq!(retained.roots,edited.roots);
 assert!(store.edit_project(&p.id,"empty",vec![],true).await.is_err());
 assert!(store.edit_project(&p.id,"untrusted",vec![a],false).await.is_err());
}
